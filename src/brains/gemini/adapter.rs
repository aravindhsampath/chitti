use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};
use anyhow::Result;
use std::sync::Arc;
use crate::tools::ToolRegistry;
use crate::brains::BrainEngine;
use crate::brains::gemini::Client;
use crate::brains::gemini::types::{InteractionInput, InteractionPart, FunctionResponse};
use crate::conductor::events::{BrainEvent, TurnContext};

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
    async fn process_turn(&self, context: TurnContext) -> Result<BoxStream<'static, Result<BrainEvent>>> {
        let input = if context.tool_results.is_empty() {
            InteractionInput::Text(context.prompt)
        } else {
            let mut parts = Vec::new();
            for res in context.tool_results {
                parts.push(InteractionPart::FunctionResponse(FunctionResponse {
                    id: Some(res.call_id),
                    name: res.name,
                    response: res.result,
                }));
            }
            // If there's a steering prompt, add it as a text part
            if !context.prompt.is_empty() {
                parts.push(InteractionPart::Text { text: context.prompt });
            }
            InteractionInput::Parts(parts)
        };

        let mut builder = self.client.interaction(input)
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
                            crate::brains::gemini::types::InteractionEvent::ContentDelta { delta, .. } => {
                                match delta {
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
                                    _ => Ok(BrainEvent::Complete { interaction_id: None }),
                                }
                            }
                            crate::brains::gemini::types::InteractionEvent::InteractionComplete { interaction } => {
                                Ok(BrainEvent::Complete { interaction_id: interaction.id })
                            }
                            _ => Ok(BrainEvent::Complete { interaction_id: None }),
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
                    crate::brains::gemini::types::InteractionOutput::ContentDelta { text, thought } => {
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
            events.push(Ok(BrainEvent::Complete { interaction_id: response.id }));
            Ok(Box::pin(futures_util::stream::iter(events)))
        }
    }
}
