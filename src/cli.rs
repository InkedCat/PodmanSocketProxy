use clap::{Args, Parser, Subcommand};

const DEFAULT_PODMAN_PATH: &str = "/run/docker.sock";
const DEFAULT_SOCKET_PATH: &str = "/var/run/safe-podman.sock";
const DEFAULT_SOCKET_IP: &str = "127.0.0.1";
const DEFAULT_SOCKET_PORT: u16 = 8787;

/// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "Podman Socket Proxy")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// The full path to the Podman socket
    #[arg(short, long, default_value_t = DEFAULT_PODMAN_PATH.to_string())]
    pub podman_path: String,

    /// The path to the TOML configuration file
    #[arg(short, long, default_value_t = String::from("./config.toml"))]
    pub config_path: String,

    #[command(subcommand)]
    pub proxy: Proxy,
}
#[derive(Subcommand, Debug)]
pub enum Proxy {
    /// Start the proxy as a Unix Socket
    Unix(UnixProxyArgs),

    /// Start the proxy as a TCP Socket
    Inet(InetProxyArgs),
}

#[derive(Args, Debug)]
pub struct UnixProxyArgs {
    /// The full path of the protected socket
    #[arg(short, long, default_value_t = DEFAULT_SOCKET_PATH.to_string())]
    pub socket_path: String,

    /// Replace the socket file if it already exists
    #[arg(short, long, default_value_t = false)]
    pub replace: bool,
}

#[derive(Args, Debug)]
pub struct InetProxyArgs {
    /// The IP address the protected socket will listen on
    #[arg(short, long, default_value_t = DEFAULT_SOCKET_IP.to_string())]
    pub ip: String,

    /// The port the protected socket will listen on
    #[arg(short, long, default_value_t = DEFAULT_SOCKET_PORT, value_parser = clap::value_parser!(u16).range(1..65535))]
    pub port: u16,
}

/// Parse the command line arguments
pub fn get_args() -> Cli {
    Cli::parse()
}
