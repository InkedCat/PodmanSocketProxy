mod cli;
mod client;
mod config;
mod errors;
mod proxy;
mod responses;

use crate::client::handle_client;
use crate::responses::request_response;
use anyhow::Context;
use proxy::connect_podman_socket;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, BufReader};

use tokio::sync::{mpsc, Semaphore};

const MAX_CONCURRENT_CONNECTIONS: usize = 10000;

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
