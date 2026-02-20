use anyhow::Result;
use tokio::sync::mpsc;
use futures_util::StreamExt;
use std::sync::Arc;
use std::collections::VecDeque;
use crate::brains::BrainEngine;
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent, BrainEvent, TurnContext, ToolResult, SessionState};
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
                        self.bridge.send(SystemEvent::Text("Context cleared.".to_string(), self.get_state_snapshot())).await?;
                    }
                    "/stream" => {
                        self.streaming = !self.streaming;
                        self.bridge.send(SystemEvent::Text(format!("Streaming is now {}", if self.streaming {"ON"} else {"OFF"}), self.get_state_snapshot())).await?;
                    }
                    "/thinking" if parts.len() > 1 => {
                        self.thinking_level = parts[1].to_string();
                        self.bridge.send(SystemEvent::Text(format!("Thinking level set to {}", self.thinking_level), self.get_state_snapshot())).await?;
                    }
                    _ => {
                        self.bridge.send(SystemEvent::Error(format!("Unknown command: {}", parts[0]), self.get_state_snapshot())).await?;
                    }
                }
            } else {
                if let Err(e) = self.handle_conversation(input).await {
                    tracing::error!("Conversation error: {:?}", e);
                    let _ = self.bridge.send(SystemEvent::Error(format!("Conversation Error: {:?}", e), self.get_state_snapshot())).await;
                    
                    // CRITICAL: On any API/Conversation error, we should probably reset the interaction ID
                    // because the current one is likely invalid for the next attempt.
                    self.previous_interaction_id = None;
                }
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
                streaming: self.streaming,
                thinking_level: self.thinking_level.clone(),
            };

            current_prompt = String::new();
            current_tool_results = Vec::new();

            let mut brain_stream = self.brain.process_turn(context).await?;
            let mut tool_calls = Vec::new();

            while let Some(brain_res) = brain_stream.next().await {
                match brain_res? {
                    BrainEvent::TextDelta(text) => {
                        self.bridge.send(SystemEvent::Text(text, self.get_state_snapshot())).await?;
                    }
                    BrainEvent::ThoughtDelta(thought) => {
                        self.bridge.send(SystemEvent::Text(format!("\x1b[2m{}\x1b[0m", thought), self.get_state_snapshot())).await?;
                    }
                    BrainEvent::ToolCall { name, id, args } => {
                        tool_calls.push((name, id, args));
                    }
                    BrainEvent::Complete { interaction_id } => {
                        if let Some(id) = interaction_id {
                            tracing::debug!(interaction_id = %id, "Turn completed, updated interaction ID");
                            self.previous_interaction_id = Some(id);
                        }
                    }
                    BrainEvent::Error(err) => {
                        self.bridge.send(SystemEvent::Error(err, self.get_state_snapshot())).await?;
                    }
                }
            }

            if tool_calls.is_empty() {
                self.bridge.send(SystemEvent::Text("\n".to_string(), self.get_state_snapshot())).await?;
                break;
            }

            // GATING: Ask for approval for all tool calls in this turn
            for (name, id, args) in tool_calls {
                let description = format!("Execute tool '{}' with args: {}", name, args);
                self.bridge.send(SystemEvent::RequestApproval { description, state: self.get_state_snapshot() }).await?;

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
                            self.bridge.send(SystemEvent::Text("[Steering noted. Waiting for tool approval/rejection...]".to_string(), self.get_state_snapshot())).await?;
                        }
                    }
                }

                if approved {
                    let args_map: std::collections::HashMap<String, serde_json::Value> = 
                        serde_json::from_value(args).unwrap_or_default();
                    
                    match self.tools.execute(&name, args_map).await {
                        Ok(res) => {
                            // Refresh metadata after tool execution
                            self.refresh_system_metadata();
                            
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
        let mut conductor = Conductor::new(brain, bridge, rx, Arc::new(ToolRegistry::new()), "test-model".to_string());

        // Turn 1
        tx.send(UserEvent::Input("ping".to_string())).await?;
        conductor.handle_conversation("ping".to_string()).await?;
        assert_eq!(conductor.previous_interaction_id, Some("id_1".to_string()));
        
        // Turn 2
        conductor.handle_conversation("pong".to_string()).await?;
        
        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].prompt, "ping");
        assert_eq!(history[0].previous_interaction_id, None);
        
        assert_eq!(history[1].prompt, "pong");
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
            "test-model".to_string()
        );

        // Start turn
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            // Wait for tool approval request
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Send steering
            tx_clone.send(UserEvent::Input("actually do X".to_string())).await.unwrap();
            // Send approval
            tx_clone.send(UserEvent::Input("y".to_string())).await.unwrap();
        });

        conductor.handle_conversation("start".to_string()).await?;

        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].prompt, "start");
        
        // Turn 2 should contain the steering text
        assert_eq!(history[1].prompt, "actually do X");
        assert_eq!(history[1].tool_results.len(), 1);
        assert_eq!(history[1].tool_results[0].name, "test_tool");

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
            "test-model".to_string()
        );

        conductor.previous_interaction_id = Some("existing".to_string());
        
        // Simulate /clear command
        tx.send(UserEvent::Input("/clear".to_string())).await?;
        
        // Run the main loop for a bit
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
            "test-model".to_string()
        );

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            // REJECT the tool
            tx_clone.send(UserEvent::Input("n".to_string())).await.unwrap();
        });

        conductor.handle_conversation("start".to_string()).await?;

        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        // Turn 2 should contain an error result for the tool
        assert_eq!(history[1].tool_results.len(), 1);
        assert!(history[1].tool_results[0].is_error);
        assert_eq!(history[1].tool_results[0].result["error"], "User rejected tool execution.");

        Ok(())
    }
}
