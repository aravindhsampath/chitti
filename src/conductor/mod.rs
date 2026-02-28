use crate::brains::gemini::types::{
    FunctionCall, FunctionResponse, InteractionInput, InteractionPart,
};
use crate::brains::BrainEngine;
use crate::bridges::CommBridge;
use crate::conductor::events::{BrainEvent, SessionState, SystemEvent, TurnContext, UserEvent};
use anyhow::Result;
use futures_util::StreamExt;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;

pub mod events;

pub struct Conductor {
    brain: Box<dyn BrainEngine>,
    bridge: Arc<dyn CommBridge>,
    events_rx: mpsc::Receiver<UserEvent>,
    interaction_id: Option<String>,
    pending_steering: VecDeque<String>,

    // UI Metadata
    streaming: bool,
    thinking_level: String,
    pwd: String,
    git_branch: String,
    dev_mode: bool,
}
impl Conductor {
    pub fn new(
        brain: Box<dyn BrainEngine>,
        bridge: Arc<dyn CommBridge>,
        events_rx: mpsc::Receiver<UserEvent>,
        dev_mode: bool,
    ) -> Self {
        let mut conductor = Self {
            brain,
            bridge,
            events_rx,
            interaction_id: None,
            pending_steering: VecDeque::new(),
            streaming: true,
            thinking_level: "high".to_string(),
            pwd: String::new(),
            git_branch: String::new(),
            dev_mode,
        };
        conductor.refresh_system_metadata();
        conductor
    }

    fn refresh_system_metadata(&mut self) {
        self.pwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        use std::process::Command;
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output();

        self.git_branch = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => "no-git".to_string(),
        };
    }
    pub fn get_state_snapshot(&self) -> SessionState {
        SessionState {
            model: "gemini-3-flash-preview".to_string(),
            thinking_level: self.thinking_level.clone(),
            streaming: self.streaming,
            memory_enabled: true,
            pwd: self.pwd.clone(),
            git_branch: self.git_branch.clone(),
            dev_mode: self.dev_mode,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(UserEvent::Input(input)) = self.events_rx.recv().await {
            if input.starts_with('/') {
                let parts: Vec<&str> = input.split_whitespace().collect();
                match parts[0] {
                    "/exit" | "/quit" => break,
                    "/clear" => {
                        self.interaction_id = None;
                        self.bridge
                            .send(SystemEvent::Info(
                                "Context cleared.".to_string(),
                                self.get_state_snapshot(),
                            ))
                            .await?;
                    }
                    "/stream" => {
                        self.streaming = !self.streaming;
                        self.bridge
                            .send(SystemEvent::Info(
                                format!(
                                    "Streaming is now {}",
                                    if self.streaming { "ON" } else { "OFF" }
                                ),
                                self.get_state_snapshot(),
                            ))
                            .await?;
                    }
                    "/thinking" if parts.len() > 1 => {
                        self.thinking_level = parts[1].to_string();
                        self.bridge
                            .send(SystemEvent::Info(
                                format!("Thinking level set to {}", self.thinking_level),
                                self.get_state_snapshot(),
                            ))
                            .await?;
                    }
                    "/help" | "/" => {
                        let help_text = "Available Commands:\n\
                              /stream          - Toggle real-time streaming\n\
                              /thinking <lvl>  - Set thinking level (minimal, low, medium, high)\n\
                              /clear           - Clear conversation context\n\
                              /exit | /quit    - Exit Chitti\n\
                              /help | /        - Show this help menu";
                        self.bridge
                            .send(SystemEvent::Info(
                                help_text.to_string(),
                                self.get_state_snapshot(),
                            ))
                            .await?;
                    }
                    _ => {
                        self.bridge
                            .send(SystemEvent::Error(
                                format!("Unknown command: {}", parts[0]),
                                self.get_state_snapshot(),
                            ))
                            .await?;
                    }
                }
            } else if let Err(e) = self.handle_conversation(input).await {
                tracing::error!("Conversation error: {:?}", e);
                let _ = self
                    .bridge
                    .send(SystemEvent::Error(
                        format!("Conversation Error: {:?}", e),
                        self.get_state_snapshot(),
                    ))
                    .await;
                self.interaction_id = None;
            }
        }
        Ok(())
    }

    async fn handle_conversation(&mut self, initial_prompt: String) -> Result<()> {
        let mut active_interaction_id = self.interaction_id.clone();
        let mut next_input = InteractionInput::Text(initial_prompt);

        loop {
            // 1. Prepare Context
            let mut parts = Vec::new();
            while let Some(steer) = self.pending_steering.pop_front() {
                parts.push(InteractionPart::Text { text: steer });
            }

            let input = if parts.is_empty() {
                next_input.clone()
            } else {
                match next_input {
                    InteractionInput::Text(t) => {
                        parts.insert(0, InteractionPart::Text { text: t });
                        InteractionInput::Parts(parts)
                    }
                    InteractionInput::Parts(mut p) => {
                        p.extend(parts);
                        InteractionInput::Parts(p)
                    }
                    InteractionInput::Turns(_) => next_input.clone(),
                }
            };

            let context = TurnContext {
                input,
                previous_interaction_id: active_interaction_id.clone(),
                streaming: self.streaming,
                thinking_level: self.thinking_level.clone(),
                memory_enabled: true,
            };
            // 3. Request Turn
            let mut brain_stream = self.brain.process_turn(context).await?;
            let mut tool_calls = Vec::new();
            let mut model_response_parts = Vec::new();

            while let Some(brain_res) = brain_stream.next().await {
                let event = brain_res?;
                match event {
                    BrainEvent::TextDelta(text) => {
                        self.bridge
                            .send(SystemEvent::Text(text.clone(), self.get_state_snapshot()))
                            .await?;
                        model_response_parts.push(InteractionPart::Text { text });
                    }
                    BrainEvent::ThoughtDelta(thought) => {
                        self.bridge
                            .send(SystemEvent::Thought(thought, self.get_state_snapshot()))
                            .await?;
                    }
                    BrainEvent::ThoughtSignature(sig) => {
                        model_response_parts.push(InteractionPart::Thought {
                            signature: sig,
                            summary: String::new(),
                        });
                    }
                    BrainEvent::ToolCall { name, id, args } => {
                        self.bridge
                            .send(SystemEvent::ToolCall {
                                name: name.clone(),
                                args: args.clone(),
                                state: self.get_state_snapshot(),
                            })
                            .await?;

                        model_response_parts.push(InteractionPart::FunctionCall(FunctionCall {
                            id: Some(id.clone()),
                            name: name.clone(),
                            args: args.clone(),
                            thought_signature: None,
                        }));

                        tool_calls.push((name, id, args));
                    }
                    BrainEvent::Complete { interaction_id } => {
                        if let Some(id) = interaction_id {
                            active_interaction_id = Some(id.clone());
                            tracing::debug!(interaction_id = %id, "Turn completed, updated session ID");
                            self.interaction_id = Some(id);
                        }
                    }
                    BrainEvent::Error(err) => {
                        self.bridge
                            .send(SystemEvent::Error(err, self.get_state_snapshot()))
                            .await?;
                    }
                }
            }

            if tool_calls.is_empty() {
                self.bridge
                    .send(SystemEvent::Ready(self.get_state_snapshot()))
                    .await?;
                break;
            }

            // 5. GATING: Tools (only dummy 'ls' supported inline)
            let mut results_parts = Vec::new();
            for (name, id, args) in tool_calls {
                let description = format!("Execute tool '{}' with args: {}", name, args);
                self.bridge
                    .send(SystemEvent::RequestApproval {
                        description,
                        state: self.get_state_snapshot(),
                    })
                    .await?;

                let mut approved = false;
                while let Some(UserEvent::Input(input)) = self.events_rx.recv().await {
                    match input.to_lowercase().as_str() {
                        "y" | "yes" => {
                            approved = true;
                            break;
                        }
                        "n" | "no" => {
                            approved = false;
                            break;
                        }
                        _ => {
                            self.pending_steering.push_back(input);
                            self.bridge
                                .send(SystemEvent::Text(
                                    "[Steering noted. Waiting for tool approval/rejection...]"
                                        .to_string(),
                                    self.get_state_snapshot(),
                                ))
                                .await?;
                        }
                    }
                }

                let result = if approved {
                    let res = if name == "ls" {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                        match std::fs::read_dir(path) {
                            Ok(entries) => {
                                let mut files = Vec::new();
                                for entry in entries.filter_map(Result::ok) {
                                    files.push(entry.file_name().to_string_lossy().to_string());
                                }
                                serde_json::json!({ "entries": files })
                            }
                            Err(e) => serde_json::json!({ "error": e.to_string() }),
                        }
                    } else {
                        serde_json::json!({ "error": format!("Tool {} not implemented", name) })
                    };
                    if self.dev_mode {
                        self.bridge
                            .send(SystemEvent::Debug(
                                format!("Tool Result: {:#?}", res),
                                self.get_state_snapshot(),
                            ))
                            .await?;
                    }
                    res
                } else {
                    serde_json::json!({ "error": "User rejected tool execution." })
                };

                results_parts.push(InteractionPart::FunctionResponse(FunctionResponse {
                    id: Some(id),
                    name,
                    response: result,
                }));
            }

            // 6. Next Input
            next_input = InteractionInput::Parts(results_parts);
        }
        Ok(())
    }
}
