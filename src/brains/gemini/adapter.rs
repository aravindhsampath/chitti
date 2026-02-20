use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};
use anyhow::Result;
use crate::brains::BrainEngine;
use crate::brains::gemini::Client;
use crate::brains::gemini::types::{InteractionInput, InteractionPart, FunctionResponse};
use crate::conductor::events::{BrainEvent, TurnContext};

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

        // Use the interaction method from the Client
        let mut builder = self.client.interaction(input);
        if let Some(id) = context.previous_interaction_id {
            builder = builder.previous_interaction_id(id);
        }

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
                                _ => Ok(BrainEvent::Complete),
                            }
                        }
                        _ => Ok(BrainEvent::Complete),
                    }
                }
                Err(e) => Err(anyhow::anyhow!("Gemini stream error: {:?}", e)),
            }
        });

        Ok(Box::pin(brain_stream))
    }
}
