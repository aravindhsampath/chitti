use crate::brains::gemini::types::{
    FunctionCall, FunctionResponse, InteractionContent, InteractionInput, InteractionPart,
    InteractionTurn, Role,
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
    memory_enabled: bool,
    turns: Vec<InteractionTurn>,
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
            memory_enabled: true,
            turns: Vec::new(),
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
            memory_enabled: self.memory_enabled,
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
                        self.turns.clear();
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
                    "/memory" => {
                        self.memory_enabled = !self.memory_enabled;
                        if !self.memory_enabled {
                            self.interaction_id = None;
                        }
                        self.bridge
                            .send(SystemEvent::Info(
                                format!(
                                    "Session memory is now {}",
                                    if self.memory_enabled { "ON" } else { "OFF" }
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
                              /memory          - Toggle session memory (privacy mode)\n\
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
        let mut active_interaction_id = if self.memory_enabled {
            self.interaction_id.clone()
        } else {
            None
        };

        let mut current_turn_history = Vec::new();
        current_turn_history.push(InteractionTurn {
            role: Role::User,
            content: InteractionContent::from(initial_prompt.clone()),
        });

        let mut next_input = InteractionInput::Text(initial_prompt);

        loop {
            while let Some(steer) = self.pending_steering.pop_front() {
                match &mut next_input {
                    InteractionInput::Text(t) => {
                        *t = format!("{}\n{}", t, steer);
                    }
                    InteractionInput::Parts(p) => {
                        p.push(InteractionPart::Text {
                            text: steer.clone(),
                        });
                    }
                    InteractionInput::Turns(turns) => {
                        if let Some(last_turn) = turns.last_mut() {
                            if last_turn.role == Role::User {
                                last_turn.content.0.push(InteractionPart::Text {
                                    text: steer.clone(),
                                });
                            }
                        }
                    }
                }
                if let Some(last_turn) = current_turn_history.last_mut() {
                    if last_turn.role == Role::User {
                        last_turn
                            .content
                            .0
                            .push(InteractionPart::Text { text: steer });
                    }
                }
            }

            let context = TurnContext {
                input: if self.memory_enabled {
                    next_input.clone()
                } else {
                    InteractionInput::Turns(current_turn_history.clone())
                },
                previous_interaction_id: active_interaction_id.clone(),
                streaming: self.streaming,
                thinking_level: self.thinking_level.clone(),
                memory_enabled: self.memory_enabled,
            };

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
                            if self.memory_enabled {
                                self.interaction_id = Some(id);
                            }
                        }
                    }
                    BrainEvent::Error(err) => {
                        self.bridge
                            .send(SystemEvent::Error(err, self.get_state_snapshot()))
                            .await?;
                    }
                }
            }

            if !model_response_parts.is_empty() {
                current_turn_history.push(InteractionTurn {
                    role: Role::Model,
                    content: InteractionContent::from(model_response_parts),
                });
            }

            if tool_calls.is_empty() {
                self.bridge
                    .send(SystemEvent::Ready(self.get_state_snapshot()))
                    .await?;
                break;
            }

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

            current_turn_history.push(InteractionTurn {
                role: Role::User,
                content: InteractionContent::from(results_parts.clone()),
            });
            next_input = InteractionInput::Parts(results_parts);
        }

        if self.memory_enabled {
            self.turns.extend(current_turn_history);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::events::{BrainEvent, SystemEvent, TurnContext, UserEvent};
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;
    use std::time::Duration;

    struct MockBrain {
        calls: Arc<Mutex<Vec<TurnContext>>>,
    }

    #[async_trait]
    impl BrainEngine for MockBrain {
        async fn process_turn(
            &self,
            context: TurnContext,
        ) -> Result<futures_util::stream::BoxStream<'static, Result<BrainEvent>>> {
            self.calls.lock().unwrap().push(context);
            let id = format!("id_{}", self.calls.lock().unwrap().len());
            Ok(Box::pin(stream::iter(vec![
                Ok(BrainEvent::TextDelta("hello".to_string())),
                Ok(BrainEvent::Complete {
                    interaction_id: Some(id),
                }),
            ])))
        }
    }

    struct TestBridge {
        sent: Arc<Mutex<Vec<SystemEvent>>>,
    }

    #[async_trait]
    impl CommBridge for TestBridge {
        async fn send(&self, event: SystemEvent) -> Result<()> {
            self.sent.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_conductor_memory_off_means_clean_slate() -> Result<()> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (_tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(MockBrain {
                calls: calls.clone(),
            }),
            Arc::new(TestBridge {
                sent: Arc::new(Mutex::new(Vec::new())),
            }),
            rx,
            false,
        );

        conductor
            .handle_conversation("I am Aravindh".to_string())
            .await?;
        assert_eq!(conductor.turns.len(), 2);

        conductor.memory_enabled = false;
        conductor
            .handle_conversation("What is my name?".to_string())
            .await?;

        {
            let history = calls.lock().unwrap();
            assert_eq!(history.len(), 2);
            if let InteractionInput::Turns(turns) = &history[1].input {
                assert_eq!(turns.len(), 1);
                if let InteractionPart::Text { text } = &turns[0].content.0[0] {
                    assert_eq!(text, "What is my name?");
                } else {
                    panic!("Expected text");
                }
            } else {
                panic!("Expected Turns input");
            }
            assert_eq!(conductor.turns.len(), 2);
        }

        conductor.memory_enabled = true;
        conductor
            .handle_conversation("Still here?".to_string())
            .await?;
        {
            let history = calls.lock().unwrap();
            assert_eq!(history.len(), 3);
            assert_eq!(conductor.turns.len(), 4);
            if let InteractionInput::Text(t) = &history[2].input {
                assert_eq!(t, "Still here?");
            } else {
                panic!("Expected Text input");
            }
        }

        Ok(())
    }

    struct ToolMockBrain {
        calls: Arc<Mutex<Vec<TurnContext>>>,
    }

    #[async_trait]
    impl BrainEngine for ToolMockBrain {
        async fn process_turn(
            &self,
            context: TurnContext,
        ) -> Result<futures_util::stream::BoxStream<'static, Result<BrainEvent>>> {
            self.calls.lock().unwrap().push(context);
            if self.calls.lock().unwrap().len() == 1 {
                Ok(Box::pin(stream::iter(vec![
                    Ok(BrainEvent::ToolCall {
                        name: "test_tool".to_string(),
                        id: "call_1".to_string(),
                        args: serde_json::json!({}),
                    }),
                    Ok(BrainEvent::Complete {
                        interaction_id: Some("id_1".to_string()),
                    }),
                ])))
            } else {
                Ok(Box::pin(stream::iter(vec![
                    Ok(BrainEvent::TextDelta("ok".to_string())),
                    Ok(BrainEvent::Complete {
                        interaction_id: Some("id_2".to_string()),
                    }),
                ])))
            }
        }
    }

    #[tokio::test]
    async fn test_conductor_privacy_mode_tool_flow() -> Result<()> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(ToolMockBrain {
                calls: calls.clone(),
            }),
            Arc::new(TestBridge {
                sent: Arc::new(Mutex::new(Vec::new())),
            }),
            rx,
            false,
        );

        conductor.memory_enabled = false;

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone
                .send(UserEvent::Input("y".to_string()))
                .await
                .unwrap();
        });

        conductor
            .handle_conversation("use tool".to_string())
            .await?;

        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);

        if let InteractionInput::Turns(t) = &history[0].input {
            assert_eq!(turns_text(t), vec!["use tool"]);
        } else {
            panic!()
        }

        if let InteractionInput::Turns(t) = &history[1].input {
            assert_eq!(t.len(), 3);
            assert_eq!(t[0].role, Role::User);
            assert_eq!(t[1].role, Role::Model);
            assert_eq!(t[2].role, Role::User);
        } else {
            panic!()
        }

        Ok(())
    }

    fn turns_text(turns: &[InteractionTurn]) -> Vec<String> {
        turns
            .iter()
            .map(|t| match &t.content.0[0] {
                InteractionPart::Text { text } => text.clone(),
                _ => "non-text".to_string(),
            })
            .collect()
    }

    #[tokio::test]
    async fn test_conductor_clear_command() -> Result<()> {
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(MockBrain {
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
            Arc::new(TestBridge {
                sent: Arc::new(Mutex::new(Vec::new())),
            }),
            rx,
            false,
        );

        conductor.interaction_id = Some("existing".to_string());
        conductor.turns.push(InteractionTurn {
            role: Role::User,
            content: InteractionContent::from("test".to_string()),
        });
        tx.send(UserEvent::Input("/clear".to_string())).await?;

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone
                .send(UserEvent::Input("/exit".to_string()))
                .await
                .unwrap();
        });

        conductor.run().await?;

        assert_eq!(conductor.interaction_id, None);
        assert!(conductor.turns.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_conductor_steering_injection() -> Result<()> {
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(ToolMockBrain {
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
            Arc::new(TestBridge {
                sent: Arc::new(Mutex::new(Vec::new())),
            }),
            rx,
            false,
        );

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone
                .send(UserEvent::Input("actually do X".to_string()))
                .await
                .unwrap();
            tx_clone
                .send(UserEvent::Input("y".to_string()))
                .await
                .unwrap();
        });

        conductor.handle_conversation("start".to_string()).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_conductor_non_streaming_flow() -> Result<()> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let brain = Box::new(MockBrain {
            calls: calls.clone(),
        });
        let sent = Arc::new(Mutex::new(Vec::new()));
        let bridge = Arc::new(TestBridge { sent: sent.clone() });
        let (_tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(brain, bridge, rx, false);
        conductor.streaming = false;

        conductor.handle_conversation("hello".to_string()).await?;

        let sent_events = sent.lock().unwrap();
        // Should have received Text event and Ready event
        let has_text = sent_events
            .iter()
            .any(|e| matches!(e, SystemEvent::Text(t, _) if t == "hello"));
        let has_ready = sent_events
            .iter()
            .any(|e| matches!(e, SystemEvent::Ready(_)));
        assert!(has_text, "Should have sent Text event");
        assert!(has_ready, "Should have sent Ready event");
        Ok(())
    }
}
