use thiserror::Error;
use crate::gemini::types::ApiError;

#[derive(Error, Debug)]
pub enum GeminiError {
    #[error("API Error: {message} (code: {code}, status: {status})")]
    Api {
        code: u16,
        message: String,
        status: String,
    },
    #[error("HTTP Error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization Error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Generic Error: {0}")]
    Other(String),
}

impl From<ApiError> for GeminiError {
    fn from(err: ApiError) -> Self {
        GeminiError::Api {
            code: err.code,
            message: err.message,
            status: err.status,
        }
    }
}

pub type Result<T> = std::result::Result<T, GeminiError>;
