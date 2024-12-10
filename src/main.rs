mod cli;
mod config;
mod errors;

use anyhow::Context;
use config::Config;
use errors::connect_podman_error::ConnectPodmanError;
use errors::open_protected_error::OpenProtectedError;
use errors::read_complete_error::ReadCompleteError;
use httparse::Request;
use std::result::Result::Ok;
use std::sync::Arc;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;

use std::{fs, os::unix::fs::FileTypeExt};

const MAX_CONCURRENT_CONNECTIONS: usize = 10000;
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024; // 10MB

enum FilterResult {
    Allowed,
    MethodNotAllowed,
    Forbidden,
    BadRequest
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

async fn send_request(
    req: &Vec<u8>,
    res: &mut Vec<u8>,
    buf_reader: &mut BufReader<UnixStream>,
) -> io::Result<()> {
    buf_reader.write_all(req).await?;
    // TODO: allow big responses
    let _size_left = buf_reader.read_buf(res).await?;

    Ok(())
}

async fn read_until_complete(
    buffer_reader: &mut BufReader<UnixStream>,
) -> Result<Vec<u8>, ReadCompleteError> {
    let mut buffer: Vec<u8> = Vec::with_capacity(10 * 1024);
    let mut is_valid = false;

    let mut temp_buffer: Vec<u8> = Vec::with_capacity(10 * 1024);
    while !is_valid {
        let size = buffer_reader.read_buf(&mut temp_buffer).await?;
        if size == 0 {
            return Err(ReadCompleteError::NoData());
        }

        if buffer.len() + size > MAX_REQUEST_SIZE {
            return Err(ReadCompleteError::ExceededMaxSize(buffer.capacity()));
        }

        buffer.append(&mut temp_buffer);

        let status = httparse::Request::new(&mut [httparse::EMPTY_HEADER; 64]).parse(&buffer)?;

        is_valid = status.is_complete();
    }

    Ok(buffer)
}

async fn write_response(
    buffer_reader: &mut BufReader<UnixStream>,
    response: &[u8],
) -> io::Result<()> {
    buffer_reader.write_all(response).await?;
    buffer_reader.flush().await?;
    Ok(())
}

async fn write_bad_request(buffer_reader: &mut BufReader<UnixStream>) -> io::Result<()> {
    let bad_request = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n400 bad request";
    write_response(buffer_reader, bad_request.as_bytes()).await?;
    Ok(())
}

async fn write_method_not_allowed(buffer_reader: &mut BufReader<UnixStream>) -> io::Result<()> {
    let forbidden = "HTTP/1.1 405 Method Not Allowed\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n405 method not allowed\n";
    write_response(buffer_reader, forbidden.as_bytes()).await?;
    Ok(())
}

async fn write_forbidden(buffer_reader: &mut BufReader<UnixStream>) -> io::Result<()> {
    let forbidden = "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nblocked by proxy\n";
    write_response(buffer_reader, forbidden.as_bytes()).await?;
    Ok(())
}

// Warning - Limitation
// No support for HTTP/2 Portocol Switching
// TODO: Implement HTTP/2 Protocol Switching
async fn handle_client(
    buf_reader: &mut BufReader<UnixStream>,
    podman_path: String,
    config: Config,
) -> anyhow::Result<()> {
    let podman_sock = connect_podman_socket(&podman_path)
        .await
        .with_context(|| format!("Failed to connect to Podman socket at {}", podman_path))?;
    let mut podman_buf_reader = BufReader::new(podman_sock);

    let mut end = false;

    while !end {
        let request_buffer = match read_until_complete(buf_reader).await {
            Ok(buffer) => buffer,
            Err(e) => {
                match &e {
                    ReadCompleteError::ReadError(_e) => {}
                    ReadCompleteError::NoData() => {}
                    _ => write_bad_request(buf_reader).await?,
                }

                end = true;
                continue;
            }
        };

        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        let result = req.parse(&request_buffer)?;

        if result.is_partial() {
            end = true;
            continue;
        } 
        
        match is_action_allowed(&req, &config) {
            FilterResult::Allowed => {
                let mut response_buf = Vec::with_capacity(10 * 1024);
                let _size_left =
                    send_request(&request_buffer, &mut response_buf, &mut podman_buf_reader).await?;
    
                write_response(buf_reader, &response_buf).await?;
            },
            FilterResult::MethodNotAllowed => {
                write_method_not_allowed(buf_reader).await?;
                end = true;
            },
            FilterResult::Forbidden => {
                write_forbidden(buf_reader).await?;
                end = true;
            },
            FilterResult::BadRequest => {
                write_bad_request(buf_reader).await?;
                end = true;
            }
        }
    }

    podman_buf_reader.shutdown().await?;
    Ok(())
}

//
// Sockets
//
async fn connect_podman_socket(podman_path: &String) -> Result<UnixStream, ConnectPodmanError> {
    if fs::exists(podman_path)? && fs::metadata(podman_path)?.file_type().is_socket() {
        let podman_sock = UnixStream::connect(podman_path).await?;
        return Ok(podman_sock);
    }

    Err(ConnectPodmanError::NoSocketFound())
}

async fn open_protected_socket(
    socket_path: &String,
    replace: bool,
) -> Result<UnixListener, OpenProtectedError> {
    if fs::metadata(socket_path).is_ok() {
        if !replace {
            return Err(OpenProtectedError::SocketExists());
        }

        fs::remove_file(socket_path)?
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
            Ok((socket, _addr)) => {
                let permit = semaphore.clone().acquire_owned().await?;

                let podman_path = args.podman_path.clone();
                let config = config.clone();
                
                let mut buf_reader = BufReader::new(socket);
                tokio::spawn(async move {
                    if let Err(e) = handle_client(&mut buf_reader, podman_path, config).await {
                        eprintln!("Error handling client: {}", e);
                    }

                    if let Err(e) = buf_reader.shutdown().await {
                        eprintln!("Error shutting down client socket: {}", e);
                    }

                    drop(buf_reader);
                    drop(permit);
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
