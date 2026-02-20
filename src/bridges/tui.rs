use async_trait::async_trait;
use tokio::sync::mpsc;
use anyhow::Result;
use std::io::{self, Write};
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent};

pub struct TuiBridge {
    tx: mpsc::Sender<UserEvent>,
}

impl TuiBridge {
    pub fn new() -> (Self, mpsc::Receiver<UserEvent>) {
        let (tx, rx) = mpsc::channel(100);
        (Self { tx }, rx)
    }

    pub async fn run_input_loop(&self) -> Result<()> {
        let stdin = io::stdin();
        loop {
            let mut user_input = String::new();
            stdin.read_line(&mut user_input)?;
            let prompt = user_input.trim();
            if prompt.is_empty() { continue; }

            match prompt.to_lowercase().as_str() {
                "y" | "yes" => {
                    self.tx.send(UserEvent::Approve).await?;
                }
                "n" | "no" => {
                    self.tx.send(UserEvent::Reject).await?;
                }
                _ if prompt.starts_with('/') => {
                    let parts: Vec<&str> = prompt.split_whitespace().collect();
                    match parts[0] {
                        "/exit" | "/quit" => {
                            self.tx.send(UserEvent::Command("/exit".to_string())).await?;
                            break;
                        }
                        "/clear" => {
                            self.tx.send(UserEvent::Command("/clear".to_string())).await?;
                        }
                        _ => {
                            self.tx.send(UserEvent::Command(prompt.to_string())).await?;
                        }
                    }
                }
                _ => {
                    // We treat normal messages as either Message or Steer 
                    // depending on Conductor state, but TuiBridge just sends Message.
                    // Conductor will decide how to handle it.
                    self.tx.send(UserEvent::Message(prompt.to_string())).await?;
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl CommBridge for TuiBridge {
    async fn send(&self, event: SystemEvent) -> Result<()> {
        let mut stdout = io::stdout();
        match event {
            SystemEvent::Text(text) => {
                print!("{}", text);
                stdout.flush()?;
            }
            SystemEvent::ToolCall { name, args } => {
                // Dimmed output for tool calls
                println!("\x1b[34m\n[Chitti calling tool: {} with args: {}]\x1b[0m", name, args);
            }
            SystemEvent::Error(err) => {
                eprintln!("\x1b[31m\n[Error: {}]\x1b[31m", err);
            }
            SystemEvent::RequestApproval { description } => {
                print!("\n\x1b[33m[Approval required: {}]\x1b[0m\nConfirm? (y/n): ", description);
                stdout.flush()?;
            }
        }
        Ok(())
    }
}
