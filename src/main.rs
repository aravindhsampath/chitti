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
use bridges::CommBridge;
use conductor::Conductor;
use tools::ToolRegistry;
use tools::bash::BashTool;

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

    // 3. Initialize Tool Registry
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(BashTool));
    let tools = Arc::new(registry);

    // 4. Initialize Components
    let client = brains::gemini::Client::new(config.gemini_api_key, config.gemini_model.clone());
    let brain = Box::new(GeminiEngine::new(client, tools.clone()));
    
    let (tui, rx) = TuiBridge::new();
    let bridge = Arc::new(tui);

    // 5. Start the Conductor
    let mut conductor = Conductor::new(brain, bridge.clone(), rx, tools.clone(), config.gemini_model);
    
    // Send an initial empty message or system event to sync the UI state
    bridge.send(crate::conductor::events::SystemEvent::Text(
        "Welcome to Chitti! Type your message or a command (e.g., /stream, /thinking, /exit).\n".to_string(),
        conductor.get_state_snapshot()
    )).await?;

    // Spawn Conductor
    tokio::spawn(async move {
        if let Err(e) = conductor.run().await {
            tracing::error!("Conductor error: {:?}", e);
        }
    });

    // Start UI loop (this will block until exit)
    bridge.run_ui_loop().await?;

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
