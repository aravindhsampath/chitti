use anyhow::Result;
use tokio::sync::mpsc;
use futures_util::StreamExt;
use crate::brains::BrainEngine;
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent, BrainEvent, TurnContext};
use std::sync::Arc;

pub mod events;
pub mod session;

pub struct Conductor {
    brain: Box<dyn BrainEngine>,
    bridge: Arc<dyn CommBridge>,
    events_rx: mpsc::Receiver<UserEvent>,
    previous_interaction_id: Option<String>,
}

impl Conductor {
    pub fn new(
        brain: Box<dyn BrainEngine>, 
        bridge: Arc<dyn CommBridge>, 
        events_rx: mpsc::Receiver<UserEvent>
    ) -> Self {
        Self {
            brain,
            bridge,
            events_rx,
            previous_interaction_id: None,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(evt) = self.events_rx.recv().await {
            match evt {
                UserEvent::Message(prompt) => {
                    self.handle_turn(prompt).await?;
                }
                UserEvent::Command(cmd) => {
                    if cmd == "/exit" {
                        break;
                    }
                    if cmd == "/clear" {
                        self.previous_interaction_id = None;
                        self.bridge.send(SystemEvent::Text("Context cleared.".to_string())).await?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn handle_turn(&mut self, prompt: String) -> Result<()> {
        let context = TurnContext {
            prompt,
            previous_interaction_id: self.previous_interaction_id.clone(),
            tool_results: Vec::new(),
        };

        let mut brain_stream = self.brain.process_turn(context).await?;

        while let Some(brain_res) = brain_stream.next().await {
            match brain_res? {
                BrainEvent::TextDelta(text) => {
                    self.bridge.send(SystemEvent::Text(text)).await?;
                }
                BrainEvent::ThoughtDelta(thought) => {
                    self.bridge.send(SystemEvent::Text(format!("\x1b[2m{}\x1b[0m", thought))).await?;
                }
                BrainEvent::ToolCall { name, args, .. } => {
                    self.bridge.send(SystemEvent::ToolCall { name, args }).await?;
                }
                BrainEvent::Complete => {
                    // Turn finished
                }
                BrainEvent::Error(err) => {
                    self.bridge.send(SystemEvent::Error(err)).await?;
                }
            }
        }

        Ok(())
    }
}
