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
    info!("Entering Chitti Lab TUI. Press Ctrl+C to exit.");
    println!("--- Chitti Lab: Test all the bells and whistles ---");
    println!("Commands: /stream, /thinking <level>, /search, /code, /state, /clear, /exit, /upload <path>");
    
    use std::io::{self, Write};
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    
    let mut use_stream = true;
    let mut thinking_level = gemini::ThinkingLevel::High;
    let mut use_search = false;
    let mut use_code = false;
    let mut is_stateful = true;
    let mut last_interaction_id: Option<String> = None;
    let mut pending_files: Vec<gemini::MediaPart> = Vec::new();

    loop {
        print!("chitti [S:{} T:{:?} G:{} C:{} H:{}]> ", 
            if use_stream {"on"} else {"off"},
            thinking_level,
            if use_search {"on"} else {"off"},
            if use_code {"on"} else {"off"},
            if is_stateful {"on"} else {"off"}
        );
        stdout.flush()?;
        
        let mut user_input = String::new();
        stdin.read_line(&mut user_input)?;
        let prompt = user_input.trim();
        if prompt.is_empty() { continue; }

        if prompt.starts_with('/') {
            let parts: Vec<&str> = prompt.split_whitespace().collect();
            match parts[0] {
                "/exit" | "/quit" => break,
                "/stream" => {
                    use_stream = !use_stream;
                    println!("Streaming is now {}", if use_stream {"ON"} else {"OFF"});
                }
                "/thinking" if parts.len() > 1 => {
                    thinking_level = match parts[1].to_lowercase().as_str() {
                        "low" => gemini::ThinkingLevel::Low,
                        "minimal" => gemini::ThinkingLevel::Minimal,
                        "medium" => gemini::ThinkingLevel::Medium,
                        _ => gemini::ThinkingLevel::High,
                    };
                    println!("Thinking level set to {:?}", thinking_level);
                }
                "/search" => {
                    use_search = !use_search;
                    println!("Google Search grounding is now {}", if use_search {"ON"} else {"OFF"});
                }
                "/code" => {
                    use_code = !use_code;
                    println!("Code Execution is now {}", if use_code {"ON"} else {"OFF"});
                }
                "/state" => {
                    is_stateful = !is_stateful;
                    if !is_stateful { last_interaction_id = None; }
                    println!("Stateful mode is now {}", if is_stateful {"ON"} else {"OFF"});
                }
                "/clear" => {
                    last_interaction_id = None;
                    pending_files.clear();
                    println!("Context cleared.");
                }
                "/upload" if parts.len() > 1 => {
                    println!("Uploading {}...", parts[1]);
                    /*
                    // Placeholder for upload logic if the method existed, but we commented it out or it's unused.
                    // If upload_file is available, we'd use it.
                    match client.upload_file(parts[1], None).await {
                        Ok(file) => {
                            println!("File uploaded: {}", file.uri);
                            pending_files.push(gemini::MediaPart {
                                uri: Some(file.uri),
                                data: None,
                                mime_type: file.mime_type,
                            });
                        }
                        Err(e) => error!("Upload failed: {:?}", e),
                    }
                    */
                    println!("Upload feature pending implementation.");
                }
                _ => println!("Unknown command."),
            }
            continue;
        }

        // Build Request
        let input = if pending_files.is_empty() {
            gemini::InteractionInput::Text(prompt.to_string())
        } else {
            let mut parts = vec![gemini::Part::Text { text: prompt.to_string() }];
            for media in pending_files.drain(..) {
                let part = if media.mime_type.starts_with("image/") {
                    gemini::Part::Image(media)
                } else if media.mime_type.starts_with("audio/") {
                    gemini::Part::Audio(media)
                } else if media.mime_type.starts_with("video/") {
                    gemini::Part::Video(media)
                } else {
                    gemini::Part::Document(media)
                };
                parts.push(part);
            }
            gemini::InteractionInput::Parts(parts)
        };

        let mut builder = client.interaction(input)
            .thinking_level(thinking_level.clone())
            .store(true);

        if let Some(ref id) = last_interaction_id {
            if is_stateful {
                builder = builder.previous_interaction_id(id.clone());
            }
        }

        let mut tools = Vec::new();
        if use_search { tools.push(gemini::Tool::GoogleSearch); }
        if use_code { tools.push(gemini::Tool::CodeExecution); }
        if !tools.is_empty() { builder = builder.tools(tools); }

        if use_stream {
            use futures_util::StreamExt;
            match builder.stream().await {
                Ok(stream) => {
                    tokio::pin!(stream);
                    print!("\nchitti: ");
                    stdout.flush()?;
                    while let Some(event_res) = stream.next().await {
                        match event_res {
                            Ok(gemini::InteractionEvent::ContentDelta { delta, .. }) => {
                                match delta {
                                    gemini::InteractionOutput::Text { text } => {
                                        print!("{}", text);
                                    }
                                    gemini::InteractionOutput::ContentDelta { text, thought } => {
                                        if thought.unwrap_or(false) {
                                            print!("\x1b[2m{}\x1b[0m", text);
                                        } else {
                                            print!("{}", text);
                                        }
                                    }
                                    _ => {}
                                }
                                stdout.flush()?;
                            }
                            Ok(gemini::InteractionEvent::InteractionComplete { interaction }) => {
                                if let Some(id) = interaction.id {
                                    last_interaction_id = Some(id);
                                }
                                println!();
                            }
                            Err(e) => {
                                println!("\nStream Error: {:?}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                    println!();
                }
                Err(e) => error!("Stream initialization failed: {:?}", e),
            }
        } else {
            match builder.send().await {
                Ok(response) => {
                    if let Some(id) = response.id {
                        last_interaction_id = Some(id);
                    }
                    print!("\nchitti: ");
                    for output in response.outputs {
                        match output {
                            gemini::InteractionOutput::Text { text } => print!("{}", text),
                            gemini::InteractionOutput::FunctionCall(fc) => print!("\n[Tool Call: {}]", fc.name),
                            _ => {}
                        }
                    }
                    println!("\n");
                }
                Err(e) => error!("API request failed: {:?}", e),
            }
        }
    }
    Ok(())
}
