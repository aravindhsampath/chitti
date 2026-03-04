use crate::brains::gemini::types::{
    FunctionCall, FunctionResponse, InteractionContent, InteractionInput, InteractionPart,
    InteractionTurn, Role,
};
use crate::brains::gemini::Client;
use crate::brains::BrainEngine;
use crate::conductor::events::{
    BrainEvent, ConversationInput, MessagePart, MessageRole, ToolCall, ToolCallPayload,
    ToolResponsePayload, TurnContext,
};
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
        let interaction_input = match &context.input {
            ConversationInput::Text(t) => InteractionInput::Text(t.clone()),
            ConversationInput::Parts(p) => {
                InteractionInput::Parts(p.iter().filter_map(convert_part).collect())
            }
            ConversationInput::Turns(turns) => InteractionInput::Turns(
                turns
                    .iter()
                    .map(|t| InteractionTurn {
                        role: match t.role {
                            MessageRole::User => Role::User,
                            MessageRole::Model => Role::Model,
                            MessageRole::Tool => Role::Tool,
                        },
                        content: InteractionContent(
                            t.parts.iter().filter_map(convert_part).collect(),
                        ),
                    })
                    .collect(),
            ),
        };

        let mut builder = self
            .client
            .interaction(interaction_input)
            .store(context.memory_enabled)
            .thinking_level(match context.thinking_level.to_lowercase().as_str() {
                "minimal" => crate::brains::gemini::types::ThinkingLevel::Minimal,
                "low" => crate::brains::gemini::types::ThinkingLevel::Low,
                "medium" => crate::brains::gemini::types::ThinkingLevel::Medium,
                _ => crate::brains::gemini::types::ThinkingLevel::High,
            });

        if let Some(instruction) = context.system_instruction {
            builder = builder.system_instruction(
                crate::brains::gemini::types::InteractionContent::from(instruction),
            );
        }

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
                                        let payload = if fc.name == "ls" {
                                            ToolCallPayload::Ls {
                                                path: fc.args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                            }
                                        } else {
                                            ToolCallPayload::Unknown {
                                                name: fc.name,
                                                raw_args: serde_json::to_string(&fc.args).unwrap_or_default(),
                                            }
                                        };
                                        Ok(BrainEvent::ToolCall(ToolCall {
                                            id: fc.id.unwrap_or_default(),
                                            payload,
                                        }))
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
                        if !text.is_empty() {
                            events.push(Ok(BrainEvent::TextDelta(text)));
                        }
                    }
                    crate::brains::gemini::types::InteractionOutput::Thought {
                        signature, ..
                    } => {
                        events.push(Ok(BrainEvent::ThoughtSignature(signature)));
                    }
                    crate::brains::gemini::types::InteractionOutput::ThoughtSignature {
                        signature,
                    } => {
                        events.push(Ok(BrainEvent::ThoughtSignature(signature)));
                    }
                    crate::brains::gemini::types::InteractionOutput::ContentDelta {
                        text,
                        thought,
                    } => {
                        if !text.is_empty() {
                            if thought.unwrap_or(false) {
                                events.push(Ok(BrainEvent::ThoughtDelta(text)));
                            } else {
                                events.push(Ok(BrainEvent::TextDelta(text)));
                            }
                        }
                    }
                    crate::brains::gemini::types::InteractionOutput::FunctionCall(fc) => {
                        let payload = if fc.name == "ls" {
                            ToolCallPayload::Ls {
                                path: fc
                                    .args
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                            }
                        } else {
                            ToolCallPayload::Unknown {
                                name: fc.name,
                                raw_args: serde_json::to_string(&fc.args).unwrap_or_default(),
                            }
                        };
                        events.push(Ok(BrainEvent::ToolCall(ToolCall {
                            id: fc.id.unwrap_or_default(),
                            payload,
                        })));
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

fn convert_part(part: &MessagePart) -> Option<InteractionPart> {
    match part {
        MessagePart::Text { text } => Some(InteractionPart::Text { text: text.clone() }),
        MessagePart::Thought { signature, summary } => Some(InteractionPart::Thought {
            signature: signature.clone(),
            summary: summary.clone(),
        }),
        MessagePart::ToolCall(tc) => {
            let (name, args) = match &tc.payload {
                ToolCallPayload::Ls { path } => {
                    let mut obj = serde_json::Map::new();
                    if let Some(p) = path {
                        obj.insert("path".to_string(), serde_json::Value::String(p.clone()));
                    }
                    ("ls".to_string(), serde_json::Value::Object(obj))
                }
                ToolCallPayload::Unknown { name, raw_args } => {
                    let parsed = serde_json::from_str(raw_args).unwrap_or(serde_json::Value::Null);
                    (name.clone(), parsed)
                }
            };
            Some(InteractionPart::FunctionCall(FunctionCall {
                id: Some(tc.id.clone()),
                name,
                args,
                thought_signature: None,
            }))
        }
        MessagePart::ToolResponse(tr) => {
            let (name, response) = match &tr.payload {
                ToolResponsePayload::Ls { entries } => match entries {
                    Ok(files) => ("ls".to_string(), serde_json::json!({ "entries": files })),
                    Err(err) => ("ls".to_string(), serde_json::json!({ "error": err })),
                },
                ToolResponsePayload::Unknown { result } => (
                    "unknown".to_string(),
                    serde_json::json!({ "result": result }),
                ),
            };
            Some(InteractionPart::FunctionResponse(FunctionResponse {
                id: Some(tr.id.clone()),
                name,
                response,
            }))
        }
    }
}
