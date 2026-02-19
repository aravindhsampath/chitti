use anyhow::{Context, Result};
use dotenvy::dotenv;
use std::env;
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

mod config;
mod gemini;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize Logging
    setup_logging()?;
    info!("Starting Chitti personal assistant...");

    // 2. Load Configuration
    if let Err(e) = dotenv() {
        warn!("No .env file found or error reading it: {}. Using environment variables.", e);
    }
    
    let config = config::Config::from_env().context("Failed to load configuration")?;
    info!("Chitti initialized with model: {}", config.gemini_model);

    // 3. Start the Interaction Loop (Daemon-like behavior)
    run_interaction_loop(config).await?;

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

async fn run_interaction_loop(config: config::Config) -> Result<()> {
    let client = gemini::Client::new(config.gemini_api_key, config.gemini_model);
    
    info!("Entering terminal interaction loop. Press Ctrl+C to exit.");
    
    use std::io::{self, Write};
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("chitti> ");
        stdout.flush()?;

        let mut input = String::new();
        stdin.read_line(&mut input)?;
        let prompt = input.trim();

        if prompt.is_empty() {
            continue;
        }

        if prompt == "/exit" || prompt == "/quit" {
            info!("Shutting down Chitti...");
            break;
        }

        info!("Sending prompt to Gemini: {}", prompt);
        
        // Use a match to handle potential API errors gracefully without crashing the daemon
        match client.generate_content(prompt).await {
            Ok(response) => {
                println!("\nchitti: {}\n", response);
            },
            Err(e) => {
                error!("Error communicating with Gemini: {:?}", e);
                println!("chitti: Sorry, I encountered an error. Please check the logs.");
            }
        }
    }

    Ok(())
}
