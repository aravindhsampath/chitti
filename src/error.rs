use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChittiError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("API request failed: {0}")]
    Api(String),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Unknown error: {0}")]
    Unknown(String),
}
