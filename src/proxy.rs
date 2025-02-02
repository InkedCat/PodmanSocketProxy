use std::os::unix::fs::FileTypeExt;

use crate::cli;
use crate::errors::ConnectPodmanError;
use crate::errors::OpenSocketError;

use cli::{TcpArgs, UnixArgs};
use tokio::fs;
use tokio::net::{TcpListener, UnixListener, UnixStream};

pub async fn connect_podman_socket(podman_path: &String) -> Result<UnixStream, ConnectPodmanError> {
    if fs::try_exists(podman_path).await?
        && fs::metadata(podman_path).await?.file_type().is_socket()
    {
        let podman_sock = UnixStream::connect(podman_path).await?;
        return Ok(podman_sock);
    }

    Err(ConnectPodmanError::NoSocketFound())
}

pub async fn open_protected_socket(args: &UnixArgs) -> Result<UnixListener, OpenSocketError> {
    if fs::metadata(&args.socket_path).await.is_ok() {
        if !args.replace {
            return Err(OpenSocketError::SocketExists());
        }

        fs::remove_file(&args.socket_path).await?
    }

    let protected_socket: UnixListener = UnixListener::bind(&args.socket_path)?;
    Ok(protected_socket)
}

pub async fn open_protected_stream(args: &TcpArgs) -> Result<TcpListener, OpenSocketError> {
    let protected_socket = TcpListener::bind(format!("{}:{}", args.ip, args.port)).await?;
    Ok(protected_socket)
}
