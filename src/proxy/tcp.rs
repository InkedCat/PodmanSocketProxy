use tokio::net::TcpListener;

use crate::{cli::InetProxyArgs, errors::OpenInetError};

pub async fn open_inet_socket(args: &InetProxyArgs) -> Result<TcpListener, OpenInetError> {
    let address = format!("{}:{}", args.ip, args.port);
    let listener = TcpListener::bind(address).await?;

    Ok(listener)
}
