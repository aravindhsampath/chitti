use anyhow::{Context, Result};
use std::env;

#[derive(Clone)]
pub struct Config {
    pub gemini_api_key: String,
    pub gemini_model: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("GEMINI_API_KEY")
            .context("GEMINI_API_KEY must be set in .env or environment")?;
        
        let model = env::var("GEMINI_MODEL")
            .unwrap_or_else(|_| "gemini-1.5-flash".to_string());

        Ok(Self {
            gemini_api_key: api_key,
            gemini_model: model,
        })
    }
}
