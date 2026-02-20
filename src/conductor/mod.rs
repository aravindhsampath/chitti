use anyhow::Result;
use tokio::sync::mpsc;
use futures_util::StreamExt;
use std::sync::Arc;
use std::collections::VecDeque;
use crate::brains::BrainEngine;
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent, BrainEvent, TurnContext, ToolResult};
use crate::tools::ToolRegistry;

pub mod events;
pub mod session;


pub struct Conductor {
    brain: Box<dyn BrainEngine>,
    bridge: Arc<dyn CommBridge>,
    events_rx: mpsc::Receiver<UserEvent>,
    tools: Arc<ToolRegistry>,
    previous_interaction_id: Option<String>,
    pending_steering: VecDeque<String>,
}

impl Conductor {
    pub fn new(
        brain: Box<dyn BrainEngine>, 
        bridge: Arc<dyn CommBridge>, 
        events_rx: mpsc::Receiver<UserEvent>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            brain,
            bridge,
            events_rx,
            tools,
            previous_interaction_id: None,
            pending_steering: VecDeque::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(evt) = self.events_rx.recv().await {
            match evt {
                UserEvent::Message(prompt) => {
                    self.handle_conversation(prompt).await?;
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

    async fn handle_conversation(&mut self, initial_prompt: String) -> Result<()> {
        let mut current_prompt = initial_prompt;
        let mut current_tool_results = Vec::new();

        loop {
            // Process any buffered steering
            while let Some(steer) = self.pending_steering.pop_front() {
                if !current_prompt.is_empty() {
                    current_prompt.push_str("\n");
                }
                current_prompt.push_str(&steer);
            }

            let context = TurnContext {
                prompt: current_prompt.clone(),
                previous_interaction_id: self.previous_interaction_id.clone(),
                tool_results: current_tool_results,
            };

            current_prompt = String::new();
            current_tool_results = Vec::new();

            let mut brain_stream = self.brain.process_turn(context).await?;
            let mut tool_calls = Vec::new();

            while let Some(brain_res) = brain_stream.next().await {
                match brain_res? {
                    BrainEvent::TextDelta(text) => {
                        self.bridge.send(SystemEvent::Text(text)).await?;
                    }
                    BrainEvent::ThoughtDelta(thought) => {
                        self.bridge.send(SystemEvent::Text(format!("\x1b[2m{}\x1b[0m", thought))).await?;
                    }
                    BrainEvent::ToolCall { name, id, args } => {
                        tool_calls.push((name, id, args));
                    }
                    BrainEvent::Complete => {}
                    BrainEvent::Error(err) => {
                        self.bridge.send(SystemEvent::Error(err)).await?;
                    }
                }
            }

            if tool_calls.is_empty() {
                self.bridge.send(SystemEvent::Text("\n".to_string())).await?;
                break;
            }

            // GATING: Ask for approval for all tool calls in this turn
            for (name, id, args) in tool_calls {
                let description = format!("Execute tool '{}' with args: {}", name, args);
                self.bridge.send(SystemEvent::RequestApproval { description }).await?;

                // Wait for Approve, Reject, or Steering
                let mut approved = false;
                while let Some(user_evt) = self.events_rx.recv().await {
                    match user_evt {
                        UserEvent::Approve => {
                            approved = true;
                            break;
                        }
                        UserEvent::Reject => {
                            approved = false;
                            break;
                        }
                        UserEvent::Message(msg) | UserEvent::Steer(msg) => {
                            self.pending_steering.push_back(msg);
                            // We keep waiting for approval/rejection of the tool, 
                            // but we've noted the steering for the next turn.
                            self.bridge.send(SystemEvent::Text("[Steering noted. Waiting for tool approval/rejection...]".to_string())).await?;
                        }
                        _ => {}
                    }
                }

                if approved {
                    let args_map: std::collections::HashMap<String, serde_json::Value> = 
                        serde_json::from_value(args).unwrap_or_default();
                    
                    match self.tools.execute(&name, args_map).await {
                        Ok(res) => {
                            current_tool_results.push(ToolResult {
                                call_id: id,
                                name,
                                result: res.output,
                                is_error: res.is_error,
                            });
                        }
                        Err(e) => {
                            current_tool_results.push(ToolResult {
                                call_id: id,
                                name,
                                result: serde_json::json!({ "error": e.to_string() }),
                                is_error: true,
                            });
                        }
                    }
                } else {
                    current_tool_results.push(ToolResult {
                        call_id: id,
                        name,
                        result: serde_json::json!({ "error": "User rejected tool execution." }),
                        is_error: true,
                    });
                }
            }

            if let Some(steer) = self.pending_steering.pop_front() {
                current_prompt = steer;
            }
        }

        Ok(())
    }
}
