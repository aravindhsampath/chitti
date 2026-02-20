use anyhow::Result;
use tokio::sync::mpsc;
use futures_util::StreamExt;
use std::sync::Arc;
use std::collections::VecDeque;
use crate::brains::BrainEngine;
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent, BrainEvent, TurnContext, SessionState};
use crate::brains::gemini::types::{InteractionInput, InteractionPart, FunctionResponse, InteractionTurn, Role, InteractionContent, FunctionCall};
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
    
    // Session State
    model: String,
    streaming: bool,
    thinking_level: String,
    memory_enabled: bool,
    dev_mode: bool,
    pwd: String,
    git_branch: String,
}

impl Conductor {
    pub fn new(
        brain: Box<dyn BrainEngine>, 
        bridge: Arc<dyn CommBridge>, 
        events_rx: mpsc::Receiver<UserEvent>,
        tools: Arc<ToolRegistry>,
        model: String,
        dev_mode: bool,
    ) -> Self {
        let mut conductor = Self {
            brain,
            bridge,
            events_rx,
            tools,
            previous_interaction_id: None,
            pending_steering: VecDeque::new(),
            model,
            streaming: true,
            thinking_level: "high".to_string(),
            memory_enabled: true,
            dev_mode,
            pwd: String::new(),
            git_branch: String::new(),
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
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout).trim().to_string()
            }
            _ => "no-git".to_string(),
        };
    }

    pub fn get_state_snapshot(&self) -> SessionState {
        SessionState {
            model: self.model.clone(),
            thinking_level: self.thinking_level.clone(),
            streaming: self.streaming,
            memory_enabled: self.memory_enabled,
            dev_mode: self.dev_mode,
            pwd: self.pwd.clone(),
            git_branch: self.git_branch.clone(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(UserEvent::Input(input)) = self.events_rx.recv().await {
            if input.starts_with('/') {
                let parts: Vec<&str> = input.split_whitespace().collect();
                match parts[0] {
                    "/exit" | "/quit" => break,
                    "/clear" => {
                        self.previous_interaction_id = None;
                        self.bridge.send(SystemEvent::Info("Context cleared.".to_string(), self.get_state_snapshot())).await?;
                    }
                    "/stream" => {
                        self.streaming = !self.streaming;
                        self.bridge.send(SystemEvent::Info(format!("Streaming is now {}", if self.streaming {"ON"} else {"OFF"}), self.get_state_snapshot())).await?;
                    }
                    "/thinking" if parts.len() > 1 => {
                        self.thinking_level = parts[1].to_string();
                        self.bridge.send(SystemEvent::Info(format!("Thinking level set to {}", self.thinking_level), self.get_state_snapshot())).await?;
                    }
                    "/memory" => {
                        self.memory_enabled = !self.memory_enabled;
                        if !self.memory_enabled {
                            self.previous_interaction_id = None;
                        }
                        self.bridge.send(SystemEvent::Info(format!("Session memory is now {}", if self.memory_enabled {"ON"} else {"OFF"}), self.get_state_snapshot())).await?;
                    }
                    "/help" | "/" => {
                        let help_text = "Available Commands:\n\
                              /stream          - Toggle real-time streaming\n\
                              /thinking <lvl>  - Set thinking level (minimal, low, medium, high)\n\
                              /memory          - Toggle session memory (privacy mode)\n\
                              /clear           - Clear conversation context\n\
                              /exit | /quit    - Exit Chitti\n\
                              /help | /        - Show this help menu";
                        self.bridge.send(SystemEvent::Info(help_text.to_string(), self.get_state_snapshot())).await?;
                    }
                    _ => {
                        self.bridge.send(SystemEvent::Error(format!("Unknown command: {}", parts[0]), self.get_state_snapshot())).await?;
                    }
                }
            } else {
                if let Err(e) = self.handle_conversation(input).await {
                    tracing::error!("Conversation error: {:?}", e);
                    let _ = self.bridge.send(SystemEvent::Error(format!("Conversation Error: {:?}", e), self.get_state_snapshot())).await;
                    self.previous_interaction_id = None;
                }
            }
        }
        Ok(())
    }

    async fn handle_conversation(&mut self, initial_prompt: String) -> Result<()> {
        let mut turn_history: Vec<InteractionTurn> = Vec::new();
        // The active_interaction_id tracks the parent ID for follow-ups *within* this interaction loop.
        let mut active_interaction_id = if self.memory_enabled { self.previous_interaction_id.clone() } else { None };
        
        let mut next_input = InteractionInput::Text(initial_prompt);

        loop {
            // 1. Incorporate steering
            while let Some(steer) = self.pending_steering.pop_front() {
                turn_history.push(InteractionTurn {
                    role: Role::User,
                    content: InteractionContent::from(steer),
                });
            }

            // 2. Prepare Context
            let context = TurnContext {
                input: if self.memory_enabled {
                    next_input.clone()
                } else {
                    // Stateless Replay: append current next_input to turn_history
                    match &next_input {
                        InteractionInput::Text(t) => {
                            turn_history.push(InteractionTurn {
                                role: Role::User,
                                content: InteractionContent::from(t.clone()),
                            });
                        }
                        InteractionInput::Parts(p) => {
                            turn_history.push(InteractionTurn {
                                role: Role::User,
                                content: InteractionContent::from(p.clone()),
                            });
                        }
                        _ => {}
                    }
                    InteractionInput::Turns(turn_history.clone())
                },
                previous_interaction_id: if self.memory_enabled { active_interaction_id.clone() } else { None },
                streaming: self.streaming,
                thinking_level: self.thinking_level.clone(),
                memory_enabled: self.memory_enabled,
                dev_mode: self.dev_mode,
            };

            if self.dev_mode {
                self.bridge.send(SystemEvent::Debug(format!("TurnContext Sent: {:#?}", context), self.get_state_snapshot())).await?;
            }

            // 3. Request Turn
            let mut brain_stream = self.brain.process_turn(context).await?;
            let mut tool_calls = Vec::new();
            let mut model_response_parts = Vec::new();

            while let Some(brain_res) = brain_stream.next().await {
                let event = brain_res?;
                if self.dev_mode {
                    self.bridge.send(SystemEvent::Debug(format!("Brain Event: {:#?}", event), self.get_state_snapshot())).await?;
                }

                match event {
                    BrainEvent::TextDelta(text) => {
                        self.bridge.send(SystemEvent::Text(text.clone(), self.get_state_snapshot())).await?;
                        model_response_parts.push(InteractionPart::Text { text });
                    }
                    BrainEvent::ThoughtDelta(thought) => {
                        self.bridge.send(SystemEvent::Thought(thought, self.get_state_snapshot())).await?;
                    }
                    BrainEvent::ThoughtSignature(sig) => {
                        model_response_parts.push(InteractionPart::Thought { 
                            signature: sig, 
                            summary: String::new() 
                        });
                    }
                    BrainEvent::ToolCall { name, id, args } => {
                        self.bridge.send(SystemEvent::ToolCall { name: name.clone(), args: args.clone(), state: self.get_state_snapshot() }).await?;
                        
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
                                tracing::debug!(interaction_id = %id, "Turn completed, updated session ID");
                                self.previous_interaction_id = Some(id);
                            }
                        }
                    }
                    BrainEvent::Error(err) => {
                        self.bridge.send(SystemEvent::Error(err, self.get_state_snapshot())).await?;
                    }
                }
            }

            // 4. Capture model response for history
            if !self.memory_enabled && !model_response_parts.is_empty() {
                turn_history.push(InteractionTurn {
                    role: Role::Model,
                    content: InteractionContent::from(model_response_parts),
                });
            }

            if tool_calls.is_empty() {
                self.bridge.send(SystemEvent::Ready(self.get_state_snapshot())).await?;
                break;
            }

            // 5. GATING: Tools
            let mut results_parts = Vec::new();
            for (name, id, args) in tool_calls {
                let description = format!("Execute tool '{}' with args: {}", name, args);
                self.bridge.send(SystemEvent::RequestApproval { description, state: self.get_state_snapshot() }).await?;

                let mut approved = false;
                while let Some(UserEvent::Input(input)) = self.events_rx.recv().await {
                    match input.to_lowercase().as_str() {
                        "y" | "yes" => { approved = true; break; }
                        "n" | "no" => { approved = false; break; }
                        _ => {
                            self.pending_steering.push_back(input);
                            self.bridge.send(SystemEvent::Text("[Steering noted. Waiting for tool approval/rejection...]".to_string(), self.get_state_snapshot())).await?;
                        }
                    }
                }

                let result = if approved {
                    match self.tools.execute(&name, args).await {
                        Ok(res) => {
                            self.refresh_system_metadata();
                            if self.dev_mode {
                                self.bridge.send(SystemEvent::Debug(format!("Tool Result: {:#?}", res), self.get_state_snapshot())).await?;
                            }
                            res.output
                        }
                        Err(e) => serde_json::json!({ "error": e.to_string() }),
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::events::{BrainEvent, UserEvent, SystemEvent, TurnContext};
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;
    use std::time::Duration;

    struct MockBrain {
        calls: Arc<Mutex<Vec<TurnContext>>>,
    }

    #[async_trait]
    impl BrainEngine for MockBrain {
        async fn process_turn(&self, context: TurnContext) -> Result<futures_util::stream::BoxStream<'static, Result<BrainEvent>>> {
            self.calls.lock().unwrap().push(context);
            let id = format!("id_{}", self.calls.lock().unwrap().len());
            Ok(Box::pin(stream::iter(vec![
                Ok(BrainEvent::TextDelta("hello".to_string())),
                Ok(BrainEvent::Complete { interaction_id: Some(id) }),
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
    async fn test_conductor_state_persistence() -> Result<()> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let brain = Box::new(MockBrain { calls: calls.clone() });
        let sent = Arc::new(Mutex::new(Vec::new()));
        let bridge = Arc::new(TestBridge { sent: sent.clone() });
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(brain, bridge, rx, Arc::new(ToolRegistry::new()), "test-model".to_string(), false);

        tx.send(UserEvent::Input("ping".to_string())).await?;
        conductor.handle_conversation("ping".to_string()).await?;
        assert_eq!(conductor.previous_interaction_id, Some("id_1".to_string()));
        
        conductor.handle_conversation("pong".to_string()).await?;
        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        // Turn 2 should have id_1 as previous
        assert_eq!(history[1].previous_interaction_id, Some("id_1".to_string()));
        Ok(())
    }

    struct ToolMockBrain {
        calls: Arc<Mutex<Vec<TurnContext>>>,
    }

    #[async_trait]
    impl BrainEngine for ToolMockBrain {
        async fn process_turn(&self, context: TurnContext) -> Result<futures_util::stream::BoxStream<'static, Result<BrainEvent>>> {
            self.calls.lock().unwrap().push(context);
            if self.calls.lock().unwrap().len() == 1 {
                Ok(Box::pin(stream::iter(vec![
                    Ok(BrainEvent::ToolCall { name: "test_tool".to_string(), id: "call_1".to_string(), args: serde_json::json!({}) }),
                    Ok(BrainEvent::Complete { interaction_id: Some("id_1".to_string()) }),
                ])))
            } else {
                Ok(Box::pin(stream::iter(vec![
                    Ok(BrainEvent::TextDelta("ok".to_string())),
                    Ok(BrainEvent::Complete { interaction_id: Some("id_2".to_string()) }),
                ])))
            }
        }
    }

    #[tokio::test]
    async fn test_conductor_steering_injection() -> Result<()> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(ToolMockBrain { calls: calls.clone() }), 
            Arc::new(TestBridge { sent: Arc::new(Mutex::new(Vec::new())) }), 
            rx, 
            Arc::new(ToolRegistry::new()),
            "test-model".to_string(),
            false
        );

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone.send(UserEvent::Input("actually do X".to_string())).await.unwrap();
            tx_clone.send(UserEvent::Input("y".to_string())).await.unwrap();
        });

        conductor.handle_conversation("start".to_string()).await?;
        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_conductor_private_mode_stateless_replay() -> Result<()> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(ToolMockBrain { calls: calls.clone() }), 
            Arc::new(TestBridge { sent: Arc::new(Mutex::new(Vec::new())) }), 
            rx, 
            Arc::new(ToolRegistry::new()),
            "test-model".to_string(),
            false
        );

        conductor.memory_enabled = false;
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone.send(UserEvent::Input("y".to_string())).await.unwrap();
        });

        conductor.handle_conversation("start".to_string()).await?;
        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        match &history[1].input {
            InteractionInput::Turns(turns) => {
                assert_eq!(turns.len(), 3); 
                assert_eq!(turns[0].role, Role::User);
                assert_eq!(turns[1].role, Role::Model);
                assert_eq!(turns[2].role, Role::User);
            }
            _ => panic!("Expected InteractionInput::Turns for private tool follow-up"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_conductor_clear_command() -> Result<()> {
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(MockBrain { calls: Arc::new(Mutex::new(Vec::new())) }), 
            Arc::new(TestBridge { sent: Arc::new(Mutex::new(Vec::new())) }), 
            rx, 
            Arc::new(ToolRegistry::new()),
            "test-model".to_string(),
            false
        );

        conductor.previous_interaction_id = Some("existing".to_string());
        tx.send(UserEvent::Input("/clear".to_string())).await?;
        
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone.send(UserEvent::Input("/exit".to_string())).await.unwrap();
        });

        conductor.run().await?;

        assert_eq!(conductor.previous_interaction_id, None);
        Ok(())
    }

    #[tokio::test]
    async fn test_conductor_tool_rejection() -> Result<()> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(ToolMockBrain { calls: calls.clone() }), 
            Arc::new(TestBridge { sent: Arc::new(Mutex::new(Vec::new())) }), 
            rx, 
            Arc::new(ToolRegistry::new()),
            "test-model".to_string(),
            false
        );

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone.send(UserEvent::Input("n".to_string())).await.unwrap();
        });

        conductor.handle_conversation("start".to_string()).await?;

        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_conductor_help_command() -> Result<()> {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(
            Box::new(MockBrain { calls: Arc::new(Mutex::new(Vec::new())) }), 
            Arc::new(TestBridge { sent: sent.clone() }), 
            rx, 
            Arc::new(ToolRegistry::new()),
            "test-model".to_string(),
            false
        );

        tx.send(UserEvent::Input("/".to_string())).await?;
        
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone.send(UserEvent::Input("/exit".to_string())).await.unwrap();
        });

        conductor.run().await?;

        let sent_events = sent.lock().unwrap();
        let help_sent = sent_events.iter().any(|e| {
            match e {
                SystemEvent::Info(text, _) => text.contains("Available Commands"),
                _ => false,
            }
        });
        assert!(help_sent);
        Ok(())
    }
}
