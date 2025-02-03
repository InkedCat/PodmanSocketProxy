use tokio::{fs, net::UnixListener};

use crate::{cli::UnixProxyArgs, errors::OpenUnixSocketError};

pub async fn open_unix_socket(args: &UnixProxyArgs) -> Result<UnixListener, OpenUnixSocketError> {
    if fs::metadata(&args.socket_path).await.is_ok() {
        if !&args.replace {
            return Err(OpenUnixSocketError::SocketExists());
        }

        fs::remove_file(&args.socket_path).await?
    }

    let protected_socket: UnixListener = UnixListener::bind(&args.socket_path)?;
    Ok(protected_socket)
}
