use anyhow::{Context, Result};
use dotenvy::dotenv;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;
use std::env;
use std::sync::Arc;

mod config;
mod brains;
mod bridges;
mod conductor;
mod tools;

use brains::gemini::adapter::GeminiEngine;
use bridges::tui::TuiBridge;
use conductor::Conductor;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize Logging
    setup_logging()?;
    info!("Starting Chitti personal assistant (Omni-Channel Refactor)...");

    // 2. Load Configuration
    if let Err(e) = dotenv() {
        warn!("No .env file found or error reading it: {}. Using environment variables.", e);
    }
    
    let config = config::Config::from_env().context("Failed to load configuration")?;
    info!("Chitti initialized with model: {}", config.gemini_model);

    // 3. Initialize Components
    let client = brains::gemini::Client::new(config.gemini_api_key, config.gemini_model);
    let brain = Box::new(GeminiEngine::new(client));
    
    let (tui, rx) = TuiBridge::new();
    let bridge = Arc::new(tui);

    // 4. Start the Conductor
    let mut conductor = Conductor::new(brain, bridge.clone(), rx);
    
    // Spawn TUI input loop
    let tui_handle = bridge.clone();
    tokio::spawn(async move {
        if let Err(e) = tui_handle.run_input_loop().await {
            tracing::error!("TUI input loop error: {:?}", e);
        }
    });

    conductor.run().await?;

    Ok(())
}

fn setup_logging() -> Result<()> {
    let log_level_str = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let log_level = match log_level_str.to_lowercase().as_str() {
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .context("Setting default subscriber failed")?;

    Ok(())
}
