mod cli;
mod config;
mod responses;

use anyhow::Context;
use config::Config;
use httparse::Request;
use responses::{BAD_REQUEST, FORBIDDEN, NOT_ALLOWED};
use std::os::unix::fs::FileTypeExt;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, Semaphore};

const MAX_CONCURRENT_CONNECTIONS: usize = 10000;
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024; // 10MB

//
// Errors
//
#[derive(Error, Debug)]
pub enum ConnectPodmanError {
    #[error("failed to connect to Podman socket")]
    ConnectError(#[from] std::io::Error),
    #[error("no socket found at path")]
    NoSocketFound(),
}

#[derive(Error, Debug)]
pub enum OpenProtectedError {
    #[error("failed to open socket")]
    SocketError(#[from] std::io::Error),
    #[error("socket file already exists")]
    SocketExists(),
}

#[derive(Error, Debug)]
pub enum ReadCompleteError {
    #[error("failed to read from stream")]
    ReadError(#[from] std::io::Error),
    #[error("no data read from stream")]
    NoData(),
    #[error("failed to parse HTTP request")]
    ParseError(#[from] httparse::Error),
    #[error("buffer capacity exceeded")]
    ExceededMaxSize(),
}

enum FilterResult {
    Allowed,
    MethodNotAllowed,
    Forbidden,
    BadRequest,
}

//
// Clients logic
//
fn is_action_allowed(req: &Request, config: &Config) -> FilterResult {
    let method = match req.method {
        Some(method) => method,
        None => return FilterResult::MethodNotAllowed,
    };

    let path = match req.path {
        Some(path) => path,
        None => return FilterResult::Forbidden,
    };

    let proxy = match method {
        "GET" => &config.get,
        "HEAD" => &config.head,
        "POST" => &config.post,
        "PUT" => &config.put,
        "PATCH" => &config.patch,
        "DELETE" => &config.delete,
        _ => return FilterResult::BadRequest,
    };

    if proxy.allowed {
        let reg = match regex::Regex::new(&proxy.regex) {
            Ok(regex) => regex,
            Err(_) => {
                panic!("Invalid regex syntax: {}", &proxy.regex)
            }
        };

        if reg.is_match(path) {
            return FilterResult::Allowed;
        }
    }

    return FilterResult::Forbidden;
}

fn headers_contains_upgrade(headers: &[httparse::Header]) -> bool {
    for header in headers {
        if header.name.to_lowercase() == "connection" {
            return true;
        }
    }

    false
}

async fn read_request(
    buffer_reader: &mut BufReader<OwnedReadHalf>,
) -> Result<Vec<u8>, ReadCompleteError> {
    let mut buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    let mut is_valid = false;

    let mut temp_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    while !is_valid {
        let size = buffer_reader.read_buf(&mut temp_buffer).await?;
        if size == 0 {
            return Err(ReadCompleteError::NoData());
        }

        if buffer.len() + size > MAX_REQUEST_SIZE {
            return Err(ReadCompleteError::ExceededMaxSize());
        }

        buffer.append(&mut temp_buffer);

        let status = httparse::Request::new(&mut [httparse::EMPTY_HEADER; 64]).parse(&buffer)?;

        is_valid = status.is_complete();
    }

    Ok(buffer)
}

struct ClientAnswer {
    buffer: Vec<u8>,
    close: bool,
}

fn close_response(message: &str) -> ClientAnswer {
    ClientAnswer {
        buffer: message.as_bytes().to_vec(),
        close: true,
    }
}

fn request_response(buffer: Vec<u8>) -> ClientAnswer {
    ClientAnswer {
        buffer,
        close: false,
    }
}

// Warning - Limitation
// No support for HTTP/2 Portocol Switching, too risky to implement
async fn handle_client(
    protected_read: OwnedReadHalf,
    mut podman_write: OwnedWriteHalf,
    writer_channel: Sender<ClientAnswer>,
    config: Config,
) -> anyhow::Result<()> {
    let mut protected_reader = BufReader::new(protected_read);

    loop {
        let request_buffer = match read_request(&mut protected_reader).await {
            Ok(buffer) => buffer,
            Err(ReadCompleteError::NoData()) => break,
            Err(ReadCompleteError::ReadError(_)) => break,
            Err(_) => {
                writer_channel.send(close_response(BAD_REQUEST)).await?;
                break;
            }
        };

        println!("Request: {:?}", String::from_utf8_lossy(&request_buffer));

        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        let result = req.parse(&request_buffer)?;
        if result.is_partial() {
            writer_channel.send(close_response(BAD_REQUEST)).await?;
            break;
        }

        if headers_contains_upgrade(req.headers) {
            writer_channel.send(close_response(FORBIDDEN)).await?;
            break;
        }

        match is_action_allowed(&req, &config) {
            FilterResult::Allowed => {
                podman_write.write_all(&request_buffer).await?;
            }
            FilterResult::MethodNotAllowed => {
                writer_channel.send(close_response(NOT_ALLOWED)).await?;
                break;
            }
            FilterResult::Forbidden => {
                writer_channel.send(close_response(FORBIDDEN)).await?;
                break;
            }
            FilterResult::BadRequest => {
                writer_channel.send(close_response(BAD_REQUEST)).await?;
                break;
            }
        }
    }

    Ok(())
}

//
// Sockets
//
async fn connect_podman_socket(podman_path: &String) -> Result<UnixStream, ConnectPodmanError> {
    if fs::try_exists(podman_path).await?
        && fs::metadata(podman_path).await?.file_type().is_socket()
    {
        let podman_sock = UnixStream::connect(podman_path).await?;
        return Ok(podman_sock);
    }

    Err(ConnectPodmanError::NoSocketFound())
}

async fn open_protected_socket(
    socket_path: &String,
    replace: bool,
) -> Result<UnixListener, OpenProtectedError> {
    if fs::metadata(socket_path).await.is_ok() {
        if !replace {
            return Err(OpenProtectedError::SocketExists());
        }

        fs::remove_file(socket_path).await?
    }

    let protected_socket: UnixListener = UnixListener::bind(socket_path)?;
    Ok(protected_socket)
}

//
// Main entrypoint
//
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::get_args();

    let config = config::get_config(&args.config_path)
        .with_context(|| format!("Failed to parse config file at {}", &args.config_path))?;

    let listener = open_protected_socket(&args.socket_path, args.replace)
        .await
        .with_context(|| format!("Failed to open protected socket at {}", &args.socket_path))?;
    println!("Listening on {}", &args.socket_path);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let permit = semaphore.clone().acquire_owned().await?;

                let podman_path = args.podman_path.clone();
                let config = config.clone();

                let podman_sock = connect_podman_socket(&podman_path).await.with_context(|| {
                    format!("Failed to connect to Podman socket at {}", podman_path)
                })?;

                let (stream_read, mut stream_write) = stream.into_split();
                let (podman_read, podman_write) = podman_sock.into_split();

                let (tx, mut rx) = mpsc::channel(1024);
                let handler_tx = tx.clone();
                let receiver_tx = tx.clone();

                tokio::spawn(async move {
                    if let Err(e) =
                        handle_client(stream_read, podman_write, handler_tx, config).await
                    {
                        eprintln!("Error occured while handling a client: {}", e);
                    }

                    drop(permit);
                });

                tokio::spawn(async move {
                    while let Some(message) = rx.recv().await {
                        match stream_write.write_all(&message.buffer).await {
                            Ok(_) => {
                                if message.close {
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("Error writing to a client: {}", e);
                                break;
                            }
                        }
                    }
                });

                tokio::spawn(async move {
                    let mut podman_buffer_reader = BufReader::new(podman_read);

                    let mut response_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
                    loop {
                        let size = match podman_buffer_reader.read_buf(&mut response_buffer).await {
                            Ok(size) => size,
                            Err(e) => {
                                eprintln!("Error reading from the Podman socket: {}", e);
                                break;
                            }
                        };

                        if size == 0 {
                            break;
                        }

                        if let Err(e) = receiver_tx
                            .send(request_response(response_buffer.clone()))
                            .await
                        {
                            eprintln!("Error sending response to a client: {}", e);
                            break;
                        }

                        response_buffer.clear();
                    }
                });
            }
            Err(err) => {
                eprintln!("Error accepting client: {}", err);
                break;
            }
        }
    }

    Ok(())
}
