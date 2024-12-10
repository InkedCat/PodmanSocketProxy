use clap::Parser;

const DEFAULT_PODMAN_PATH: &str = "/run/podman.sock";
const DEFAULT_SOCKET_PATH: &str = "/tmp/safe-podman.sock";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]

/// CLI arguments
pub struct Args {
    /// The full path to the Podman socket
    #[arg(short, long, default_value_t = DEFAULT_PODMAN_PATH.to_string())]
    pub podman_path: String,

    /// The full path of the protected socket
    #[arg(short, long, default_value_t = DEFAULT_SOCKET_PATH.to_string())]
    pub socket_path: String,

    /// Replace the existing socket if it exists
    /// This will remove the existing socket file if it exists
    /// and create a new one
    /// If the socket does not exist, it will be created
    /// If this flag is not set and the socket exists, the program will exit
    /// with an error
    #[arg(short, long, default_value_t = false)]
    pub replace: bool,

    /// The path to the configuration file
    /// This file should be in TOML format
    /// The default value is "./config.toml"
    /// If the file does not exist, the program will exit with an error
    /// If the file is not valid TOML, the program will exit with an error
    /// If the file is valid TOML, but the regex patterns are invalid, the program will exit with an error
    #[arg(short, long, default_value_t = String::from("./config.toml"))]
    pub config_path: String,
}

/// Parse the command line arguments
pub fn get_args() -> Args {
    Args::parse()
}
