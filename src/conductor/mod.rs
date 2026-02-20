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
                    BrainEvent::Complete { interaction_id } => {
                        if let Some(id) = interaction_id {
                            self.previous_interaction_id = Some(id);
                        }
                    }
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
        
        let (_tx, rx) = mpsc::channel(10);
        let mut conductor = Conductor::new(brain, bridge, rx, Arc::new(ToolRegistry::new()));
        conductor.handle_conversation("ping".to_string()).await?;
        assert_eq!(conductor.previous_interaction_id, Some("id_1".to_string()));
        conductor.handle_conversation("pong".to_string()).await?;
        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].prompt, "ping");
        assert_eq!(history[0].previous_interaction_id, None);
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
            Arc::new(ToolRegistry::new())
        );
        // Start turn
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            // Wait for tool approval request
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Send steering
            tx_clone.send(UserEvent::Steer("actually do X".to_string())).await.unwrap();
            // Send approval
            tx_clone.send(UserEvent::Approve).await.unwrap();
        });

        conductor.handle_conversation("start".to_string()).await?;

        let history = calls.lock().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].prompt, "start");
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
            Arc::new(ToolRegistry::new())
        );

        conductor.previous_interaction_id = Some("existing".to_string());
        
        // Simulate /clear command
        tx.send(UserEvent::Command("/clear".to_string())).await?;
        
        // Run the main loop for a bit
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx_clone.send(UserEvent::Command("/exit".to_string())).await.unwrap();
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
            Arc::new(ToolRegistry::new())
        );

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            // REJECT the tool
            tx_clone.send(UserEvent::Reject).await.unwrap();
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