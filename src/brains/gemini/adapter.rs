use crate::brains::gemini::Client;
use crate::brains::BrainEngine;
use crate::conductor::events::{BrainEvent, TurnContext};
use anyhow::Result;
use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};

pub struct GeminiEngine {
    client: Client,
}

impl GeminiEngine {
    pub fn new(client: Client) -> Self {
        Self { client }
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

        // Add simple mock 'ls' tool definition to test function calling
        let ls_tool = crate::brains::gemini::types::Tool::Function {
            declaration: crate::brains::gemini::types::FunctionDeclaration {
                name: "ls".to_string(),
                description: "List files in a directory".to_string(),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                })),
            },
        };
        builder = builder.tools(vec![ls_tool]);
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
