use thiserror::Error;

#[derive(Error, Debug)]
pub enum OpenProtectedError {
    #[error("failed to open socket")]
    SocketError(#[from] std::io::Error),
    #[error("socket file already exists")]
    SocketExists(),
}
