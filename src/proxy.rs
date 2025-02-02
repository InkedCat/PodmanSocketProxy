use std::os::unix::fs::FileTypeExt;

use crate::errors::ConnectPodmanError;
use crate::errors::OpenSocketError;

use tokio::fs;
use tokio::net::{UnixListener, UnixStream};

pub async fn connect_podman_socket(podman_path: &String) -> Result<UnixStream, ConnectPodmanError> {
    if fs::try_exists(podman_path).await?
        && fs::metadata(podman_path).await?.file_type().is_socket()
    {
        let podman_sock = UnixStream::connect(podman_path).await?;
        return Ok(podman_sock);
    }

    Err(ConnectPodmanError::NoSocketFound())
}

pub async fn open_protected_socket(
    socket_path: &String,
    replace: bool,
) -> Result<UnixListener, OpenSocketError> {
    if fs::metadata(socket_path).await.is_ok() {
        if !replace {
            return Err(OpenSocketError::SocketExists());
        }

        fs::remove_file(socket_path).await?
    }

    let protected_socket: UnixListener = UnixListener::bind(socket_path)?;
    Ok(protected_socket)
}
