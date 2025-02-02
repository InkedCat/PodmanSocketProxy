use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConnectPodmanError {
    #[error("failed to connect to Podman socket")]
    ConnectError(#[from] std::io::Error),
    #[error("no socket found at path")]
    NoSocketFound(),
}

#[derive(Error, Debug)]
pub enum OpenSocketError {
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
