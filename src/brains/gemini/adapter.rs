use crate::brains::gemini::Client;
use crate::brains::BrainEngine;
use crate::conductor::events::{BrainEvent, TurnContext};
use crate::tools::ToolRegistry;
use anyhow::Result;
use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};
use std::sync::Arc;

pub struct GeminiEngine {
    client: Client,
    tools: Arc<ToolRegistry>,
}

impl GeminiEngine {
    pub fn new(client: Client, tools: Arc<ToolRegistry>) -> Self {
        Self { client, tools }
    }
}

#[async_trait]
impl BrainEngine for GeminiEngine {
    async fn process_turn(
        &self,
        context: TurnContext,
    ) -> Result<BoxStream<'static, Result<BrainEvent>>> {
        let mut builder = self
            .client
            .interaction(context.input)
            .store(context.memory_enabled)
            .thinking_level(match context.thinking_level.to_lowercase().as_str() {
                "minimal" => crate::brains::gemini::types::ThinkingLevel::Minimal,
                "low" => crate::brains::gemini::types::ThinkingLevel::Low,
                "medium" => crate::brains::gemini::types::ThinkingLevel::Medium,
                _ => crate::brains::gemini::types::ThinkingLevel::High,
            });

        if let Some(id) = context.previous_interaction_id {
            builder = builder.previous_interaction_id(id);
        }

        // Add tool definitions
        let tool_defs = self.tools.get_definitions();
        if !tool_defs.is_empty() {
            builder = builder.tools(tool_defs);
        }

        if context.streaming {
            let stream = builder.stream().await?;

            let brain_stream = stream.map(|res| {
                match res {
                    Ok(evt) => {
                        match evt {
                            crate::brains::gemini::types::InteractionEvent::InteractionStart { interaction } => {
                                if let Some(ref id) = interaction.id {
                                    if !id.is_empty() {
                                        return Ok(BrainEvent::Complete { interaction_id: Some(id.clone()) });
                                    }
                                }
                                Ok(BrainEvent::ThoughtDelta(String::new()))
                            }
                            crate::brains::gemini::types::InteractionEvent::ContentDelta { delta, .. } => {
                                match delta {
                                    crate::brains::gemini::types::InteractionOutput::Thought { signature, .. } => Ok(BrainEvent::ThoughtSignature(signature)),
                                crate::brains::gemini::types::InteractionOutput::ThoughtSignature { signature } => Ok(BrainEvent::ThoughtSignature(signature)),
                                crate::brains::gemini::types::InteractionOutput::Text { text } => Ok(BrainEvent::TextDelta(text)),
                                    crate::brains::gemini::types::InteractionOutput::ContentDelta { text, thought } => {
                                        if thought.unwrap_or(false) {
                                            Ok(BrainEvent::ThoughtDelta(text))
                                        } else {
                                            Ok(BrainEvent::TextDelta(text))
                                        }
                                    }
                                    crate::brains::gemini::types::InteractionOutput::FunctionCall(fc) => {
                                        Ok(BrainEvent::ToolCall { 
                                            name: fc.name, 
                                            id: fc.id.unwrap_or_default(), 
                                            args: serde_json::to_value(fc.args).unwrap_or_default() 
                                        })
                                    }
                                    _ => {
                                        // Skip other output types without yielding Complete
                                        // Yielding an empty ThoughtDelta or similar is better than Complete
                                        Ok(BrainEvent::ThoughtDelta(String::new()))
                                    },
                                }
                            }
                            crate::brains::gemini::types::InteractionEvent::InteractionComplete { interaction } => {
                                if let Some(ref id) = interaction.id {
                                    if !id.is_empty() {
                                        return Ok(BrainEvent::Complete { interaction_id: Some(id.clone()) });
                                    }
                                }
                                Ok(BrainEvent::Complete { interaction_id: None })
                            }
                            _ => Ok(BrainEvent::ThoughtDelta(String::new())),
                        }
                    }
                    Err(e) => Err(anyhow::anyhow!("Gemini stream error: {:?}", e)),
                }
            });

            Ok(Box::pin(brain_stream))
        } else {
            let response = builder.send().await?;
            let mut events = Vec::new();
            for output in response.outputs {
                match output {
                    crate::brains::gemini::types::InteractionOutput::Text { text } => {
                        events.push(Ok(BrainEvent::TextDelta(text)));
                    }
                    crate::brains::gemini::types::InteractionOutput::ContentDelta {
                        text,
                        thought,
                    } => {
                        if thought.unwrap_or(false) {
                            events.push(Ok(BrainEvent::ThoughtDelta(text)));
                        } else {
                            events.push(Ok(BrainEvent::TextDelta(text)));
                        }
                    }
                    crate::brains::gemini::types::InteractionOutput::FunctionCall(fc) => {
                        events.push(Ok(BrainEvent::ToolCall {
                            name: fc.name,
                            id: fc.id.unwrap_or_default(),
                            args: serde_json::to_value(fc.args).unwrap_or_default(),
                        }));
                    }
                    _ => {}
                }
            }
            events.push(Ok(BrainEvent::Complete {
                interaction_id: response.id,
            }));
            Ok(Box::pin(futures_util::stream::iter(events)))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde_json::json;
    use crate::brains::gemini::types::InteractionInput;

    #[tokio::test]
    async fn test_adapter_streaming_parsing() {
        let mut server = Server::new_async().await;
        let mock = server.mock("POST", "/v1beta/interactions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(
                "data: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\"hello \"}}\n\n\
                 data: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\"world\"}}\n\n\
                 data: {\"event_type\":\"interaction.complete\",\"interaction\":{\"id\":\"int_123\",\"model\":\"gemini-3-flash-preview\",\"status\":\"completed\",\"outputs\":[]}}\n\n\
                 data: [DONE]\n\n"
            )
            .create_async().await;

        let client = Client::new("test_key".to_string(), "gemini-3-flash-preview".to_string())
            .with_base_url(server.url());
        
        let engine = GeminiEngine::new(client, Arc::new(ToolRegistry::new()));
        
        let context = TurnContext {
            input: InteractionInput::Text("Hi".to_string()),
            previous_interaction_id: None,
            streaming: true,
            thinking_level: "high".to_string(),
            memory_enabled: true,
            dev_mode: false,
        };

        let mut stream = engine.process_turn(context).await.unwrap();
        
        let mut events = Vec::new();
        while let Some(Ok(event)) = stream.next().await {
            events.push(event);
        }
        
        assert_eq!(events.len(), 3);
        match &events[0] {
            BrainEvent::TextDelta(t) => assert_eq!(t, "hello "),
            _ => panic!("Expected TextDelta"),
        }
        match &events[1] {
            BrainEvent::TextDelta(t) => assert_eq!(t, "world"),
            _ => panic!("Expected TextDelta"),
        }
        match &events[2] {
            BrainEvent::Complete { interaction_id } => assert_eq!(interaction_id.as_deref(), Some("int_123")),
            _ => panic!("Expected Complete"),
        }
        
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_adapter_non_streaming_parsing() {
        let mut server = Server::new_async().await;
        let mock = server.mock("POST", "/v1beta/interactions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "id": "int_456",
                "model": "gemini-3-flash-preview",
                "status": "completed",
                "outputs": [{ "type": "text", "text": "Hello non-stream" }]
            }).to_string())
            .create_async().await;

        let client = Client::new("test_key".to_string(), "gemini-3-flash-preview".to_string())
            .with_base_url(server.url());
        
        let engine = GeminiEngine::new(client, Arc::new(ToolRegistry::new()));
        
        let context = TurnContext {
            input: InteractionInput::Text("Hi".to_string()),
            previous_interaction_id: None,
            streaming: false,
            thinking_level: "low".to_string(),
            memory_enabled: false,
            dev_mode: false,
        };

        let mut stream = engine.process_turn(context).await.unwrap();
        
        let mut events = Vec::new();
        while let Some(Ok(event)) = stream.next().await {
            events.push(event);
        }
        
        assert_eq!(events.len(), 2);
        match &events[0] {
            BrainEvent::TextDelta(t) => assert_eq!(t, "Hello non-stream"),
            _ => panic!("Expected TextDelta"),
        }
        match &events[1] {
            BrainEvent::Complete { interaction_id } => assert_eq!(interaction_id.as_deref(), Some("int_456")),
            _ => panic!("Expected Complete"),
        }
        
        mock.assert_async().await;
    }
}