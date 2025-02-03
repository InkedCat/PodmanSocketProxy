pub mod client;
pub mod tcp;
pub mod unix;

use tokio::io::{self, AsyncWriteExt};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net;

pub enum ProxyListener {
    Inet(net::TcpListener),
    Unix(net::UnixListener),
}

impl From<net::TcpListener> for ProxyListener {
    fn from(listener: net::TcpListener) -> Self {
        ProxyListener::Inet(listener)
    }
}

impl From<net::UnixListener> for ProxyListener {
    fn from(listener: net::UnixListener) -> Self {
        ProxyListener::Unix(listener)
    }
}

impl ProxyListener {
    pub async fn accept(&self) -> io::Result<ProxyStream> {
        match self {
            ProxyListener::Inet(listener) => {
                let (stream, _) = listener.accept().await?;
                Ok(ProxyStream::Inet(stream))
            }
            ProxyListener::Unix(listener) => {
                let (stream, _) = listener.accept().await?;
                Ok(ProxyStream::Unix(stream))
            }
        }
    }
}

pub enum ProxyStream {
    Inet(net::TcpStream),
    Unix(net::UnixStream),
}

impl From<net::TcpStream> for ProxyStream {
    fn from(stream: net::TcpStream) -> Self {
        ProxyStream::Inet(stream)
    }
}

impl From<net::UnixStream> for ProxyStream {
    fn from(stream: net::UnixStream) -> Self {
        ProxyStream::Unix(stream)
    }
}

impl ProxyStream {
    pub fn split(self) -> (ProxyBufferedRead, ProxyWriteHalf) {
        match self {
            ProxyStream::Inet(stream) => {
                let (read, write) = stream.into_split();
                (
                    ProxyBufferedRead::Inet(BufReader::new(read)),
                    ProxyWriteHalf::Inet(write),
                )
            }
            ProxyStream::Unix(stream) => {
                let (read, write) = stream.into_split();
                (
                    ProxyBufferedRead::Unix(BufReader::new(read)),
                    ProxyWriteHalf::Unix(write),
                )
            }
        }
    }
}

pub enum ProxyBufferedRead {
    Inet(tokio::io::BufReader<net::tcp::OwnedReadHalf>),
    Unix(tokio::io::BufReader<net::unix::OwnedReadHalf>),
}

impl ProxyBufferedRead {
    pub async fn read(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        match self {
            ProxyBufferedRead::Inet(read) => read.read_buf(buf).await,
            ProxyBufferedRead::Unix(read) => read.read_buf(buf).await,
        }
    }
}

pub enum ProxyWriteHalf {
    Inet(net::tcp::OwnedWriteHalf),
    Unix(net::unix::OwnedWriteHalf),
}

impl ProxyWriteHalf {
    pub async fn write(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            ProxyWriteHalf::Inet(write) => write.write_all(buf).await,
            ProxyWriteHalf::Unix(write) => write.write_all(buf).await,
        }
    }
}
