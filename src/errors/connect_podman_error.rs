use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConnectPodmanError {
    #[error("failed to connect to Podman socket")]
    ConnectError(#[from] std::io::Error),
    #[error("no socket found at path")]
    NoSocketFound(),
}
