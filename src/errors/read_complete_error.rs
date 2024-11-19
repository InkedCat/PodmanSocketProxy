use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReadCompleteError {
    #[error("failed to read from stream")]
    ReadError(#[from] std::io::Error),
    #[error("no data read from stream")]
    NoData(),
    #[error("failed to parse HTTP request")]
    ParseError(#[from] httparse::Error),
    #[error("buffer capacity exceeded")]
    ExceededMaxSize(usize),
}
