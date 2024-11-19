mod errors;

use anyhow::Context;
use clap::Parser;
use errors::connect_podman_error::ConnectPodmanError;
use errors::open_protected_error::OpenProtectedError;
use errors::read_complete_error::ReadCompleteError;
use httparse::{self, Request};
use std::result::Result::Ok;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use anyhow::bail;
use std::{fs, os::unix::fs::FileTypeExt};

const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024; // 10MB
const DEFAULT_PODMAN_PATH: &str = "/run/docker.sock";
const DEFAULT_SOCKET_PATH: &str = "/tmp/safe-docker.sock";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The full path to the Podman socket
    #[arg(short, long, default_value_t = DEFAULT_PODMAN_PATH.to_string())]
    podman_path: String,

    /// The full path of the protected socket
    #[arg(short, long, default_value_t = DEFAULT_SOCKET_PATH.to_string())]
    socket_path: String,

    /// Replace the existing socket if it exists
    /// This will remove the existing socket file if it exists
    /// and create a new one
    /// If the socket does not exist, it will be created
    /// If this flag is not set and the socket exists, the program will exit
    /// with an error
    #[arg(short, long, default_value_t = false)]
    replace: bool,
}

fn is_action_allowed(_req: &Request) -> bool {
    true
}

async fn send_request(
    req: &Vec<u8>,
    res: &mut Vec<u8>,
    podman_sock: &mut UnixStream,
) -> anyhow::Result<()> {
    podman_sock.write_all(req).await?;
    // TODO: treat possible big request
    let _size_left = podman_sock.read_buf(res).await?;

    Ok(())
}

async fn read_until_complete(stream: &mut UnixStream) -> Result<Vec<u8>, ReadCompleteError> {
    // TODO: Block reallocation of buffer
    let mut buffer: Vec<u8> = Vec::with_capacity(10 * 1024);
    let mut is_valid = false;

    let mut temp_buffer: Vec<u8> = Vec::with_capacity(10 * 1024);
    while !is_valid {
        let size = stream.read_buf(&mut temp_buffer).await?;
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

async fn write_response(stream: &mut UnixStream, response: &[u8]) -> anyhow::Result<()> {
    stream.write_all(response).await?;
    stream.flush().await?;
    Ok(())
}

// async fn write_server_error(stream: &mut UnixStream) -> anyhow::Result<()> {
//     let server_error = "HTTP/1.1 500 Internal Server Error\nContent-Type: text/plain; charset=utf-8\nConnection: close\r\n\r\n";
//     write_response(stream, server_error.as_bytes()).await?;
//     Ok(())
// }

async fn write_bad_request(stream: &mut UnixStream) -> anyhow::Result<()> {
    let bad_request = "HTTP/1.1 400 Bad Request\nContent-Type: text/plain; charset=utf-8\nConnection: close\r\n\r\n";
    write_response(stream, bad_request.as_bytes()).await?;
    Ok(())
}

async fn write_forbidden(stream: &mut UnixStream) -> anyhow::Result<()> {
    let forbidden = "HTTP/1.1 403 Forbidden\nContent-Type: text/plain; charset=utf-8\nConnection: close\r\n\r\n";
    write_response(stream, forbidden.as_bytes()).await?;
    Ok(())
}

async fn handle_client(mut stream: UnixStream, podman_path: String) -> anyhow::Result<()> {
    let mut podman_sock = connect_podman_socket(&podman_path)
        .await
        .with_context(|| format!("Failed to connect to Podman socket at {}", podman_path))?;
    println!("Connected to the Podman API socket");

    let mut end = false;

    while !end {
        let request_buffer = match read_until_complete(&mut stream).await {
            Ok(buffer) => buffer,
            Err(e) => {
                match &e {
                    ReadCompleteError::ReadError(_e) => {},
                    ReadCompleteError::NoData() => {},
                    _ => write_bad_request(&mut stream).await?,
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
        } else if is_action_allowed(&req) {
            let mut response_buf = Vec::with_capacity(10 * 1024);
            let _size_left =
                send_request(&request_buffer, &mut response_buf, &mut podman_sock).await?;

            write_response(&mut stream, &response_buf).await?;
        } else {
            write_forbidden(&mut stream).await?;
        }
    }

    podman_sock.shutdown().await?;
    stream.shutdown().await?;
    Ok(())
}

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let listener = open_protected_socket(&args.socket_path, args.replace)
        .await
        .with_context(|| format!("Failed to open protected socket at {}", &args.socket_path))?;
    println!("Listening on {}", &args.socket_path);

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let podman_path = args.podman_path.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, podman_path).await {
                        eprintln!("Socket client error: {}", e);
                    }
                });
            }
            Err(err) => {
                eprintln!("Error: {}", err);
                break;
            }
        }
    }

    Ok(())
}
