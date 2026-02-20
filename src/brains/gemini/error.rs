use thiserror::Error;
use crate::brains::gemini::types::ApiError;
use tokio_util::codec::LinesCodecError;

#[derive(Error, Debug)]
pub enum GeminiError {
    #[error("API Error: {message} (code: {code})")]
    Api {
        code: String,
        message: String,
    },
    #[error("HTTP Error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization Error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Stream Error: {0}")]
    #[allow(dead_code)]
    Stream(String),
    #[error("Codec Error: {0}")]
    Codec(#[from] LinesCodecError),
    #[error("Generic Error: {0}")]
    Other(String),
}

impl From<ApiError> for GeminiError {
    fn from(err: ApiError) -> Self {
        GeminiError::Api {
            code: err.code,
            message: err.message,
        }
    }
}

pub type Result<T> = std::result::Result<T, GeminiError>;
