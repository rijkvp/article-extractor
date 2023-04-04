use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error")]
    IO(#[from] std::io::Error),
    #[error("Unknown Error")]
    Unknown,
}
