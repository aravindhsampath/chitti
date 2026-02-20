use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub gemini_api_key: String,
    pub gemini_model: String,
    pub dev_mode: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("GEMINI_API_KEY")
            .context("GEMINI_API_KEY must be set in .env or environment")?;
        
        let model = env::var("GEMINI_MODEL")
            .unwrap_or_else(|_| "gemini-1.5-flash".to_string());

        let dev_mode = env::var("DEV_MODE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true); // Default to true as requested

        Ok(Self {
            gemini_api_key: api_key,
            gemini_model: model,
            dev_mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::env;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_from_env_success() {
        let _guard = ENV_MUTEX.lock().unwrap();
        
        env::set_var("GEMINI_API_KEY", "test-key");
        env::set_var("GEMINI_MODEL", "test-model");
        
        let config = Config::from_env().unwrap();
        assert_eq!(config.gemini_api_key, "test-key");
        assert_eq!(config.gemini_model, "test-model");
        
        env::remove_var("GEMINI_API_KEY");
        env::remove_var("GEMINI_MODEL");
    }

    #[test]
    fn test_from_env_missing_key() {
        let _guard = ENV_MUTEX.lock().unwrap();
        
        env::remove_var("GEMINI_API_KEY");
        
        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GEMINI_API_KEY must be set"));
    }
}