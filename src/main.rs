mod cli;
mod config;
mod errors;
mod filter;
mod proxy;
mod responses;

use crate::responses::request_response;
use anyhow::Context;
use errors::ConnectPodmanError;
use log;
use proxy::client::handle_client;
use std::os::unix::fs::FileTypeExt;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, Semaphore};
use env_logger::Env;

const MAX_CONCURRENT_CONNECTIONS: usize = 10000;

struct PodmanSocketConnector {
    podman_path: String,
}

impl PodmanSocketConnector {
    pub fn new(podman_path: String) -> Self {
        PodmanSocketConnector { podman_path }
    }

    pub async fn connect(&self) -> Result<UnixStream, ConnectPodmanError> {
        if fs::try_exists(&self.podman_path).await?
            && fs::metadata(&self.podman_path)
                .await?
                .file_type()
                .is_socket()
        {
            let podman_sock = UnixStream::connect(&self.podman_path).await?;
            return Ok(podman_sock);
        }

        Err(ConnectPodmanError::NoSocketFound(self.podman_path.clone()))
    }
}

//
// Main entrypoint
//
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = cli::get_args();

    let config = config::get_config(&args.config_path)
        .with_context(|| format!("Failed to parse config file at {}", &args.config_path))?;

    let listener = match args.proxy {
        cli::Proxy::Inet(args) => {
            let inet_socket = proxy::tcp::open_inet_socket(&args).await?;

            log::info!("Listening on: {}:{}", args.ip, args.port);
            proxy::ProxyListener::Inet(inet_socket)
        }
        cli::Proxy::Unix(args) => {
            let unix_socket = proxy::unix::open_unix_socket(&args).await?;

            log::info!("Listening on: {:?}", args.socket_path);
            proxy::ProxyListener::Unix(unix_socket)
        }
    };

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));
    let podman_connector = PodmanSocketConnector::new(args.podman_path.clone());
    let filters_handler = filter::FiltersHandler::new(config.filters);

    loop {
        match listener.accept().await {
            Ok(stream) => {
                log::debug!("Accepted a client connection");

                let permit = match semaphore.clone().acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        log::error!("Too many clients, client connection closed");
                        break;
                    }
                };

                let filters_handler = filters_handler.clone();

                let podman_sock = match podman_connector.connect().await {
                    Ok(sock) => sock,
                    Err(e) => {
                        log::error!("Failed to establish connection with the Podman API : {}", e);
                        break;
                    }
                };
                let (podman_read, podman_write) = podman_sock.into_split();

                let (stream_read, mut stream_write) = stream.split();

                let (tx, mut rx) = mpsc::channel(1024);
                let handler_tx = tx.clone();
                let receiver_tx = tx.clone();

                tokio::spawn(async move {
                    if let Err(e) =
                        handle_client(stream_read, podman_write, handler_tx, filters_handler).await
                    {
                        log::error!("Error occured while handling a client: {}", e);
                    }

                    drop(permit);
                });

                tokio::spawn(async move {
                    while let Some(message) = rx.recv().await {
                        match stream_write.write(&message.buffer).await {
                            Ok(_) => {
                                if message.close {
                                    break;
                                }
                            }
                            Err(e) => {
                                log::error!("Error writing to a client: {}", e);
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
                                log::error!("Error reading from the Podman socket: {}", e);
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
                            log::error!("Error sending response to a client: {}", e);
                            break;
                        }

                        response_buffer.clear();
                    }
                });
            }
            Err(err) => {
                log::error!("Error accepting client: {}", err);
                break;
            }
        }
    }

    Ok(())
}
