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
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GeminiEngine {
    client: Client,
    active_cache: Arc<RwLock<Option<(u64, String)>>>,
}

impl GeminiEngine {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            active_cache: Arc::new(RwLock::new(None)),
        }
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
            let mut hasher = DefaultHasher::new();
            instruction.hash(&mut hasher);
            let current_hash = hasher.finish();

            let mut current_cache = None;
            {
                let cache_lock = self.active_cache.read().await;
                if let Some((hash, name)) = cache_lock.as_ref() {
                    if *hash == current_hash {
                        current_cache = Some(name.clone());
                    }
                }
            }

            if current_cache.is_none() {
                let content = crate::brains::gemini::types::Content {
                    role: None,
                    parts: vec![crate::brains::gemini::types::Part {
                        text: Some(instruction.clone()),
                        ..Default::default()
                    }],
                };
                let new_cache = crate::brains::gemini::types::CachedContent {
                    name: None,
                    model: format!("models/{}", self.client.model),
                    contents: None,
                    system_instruction: Some(content),
                    tools: None,
                    ttl: Some("3600s".to_string()),
                    expire_time: None,
                };
                match self.client.create_cached_content(new_cache).await {
                    Ok(cached) => {
                        if let Some(name) = cached.name {
                            tracing::info!("Created new context cache: {}", name);
                            let mut cache_lock = self.active_cache.write().await;
                            *cache_lock = Some((current_hash, name.clone()));
                            current_cache = Some(name);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to create cache: {}. Falling back to inline.", e);
                    }
                }
            }

            if let Some(cache_name) = current_cache {
                builder = builder.cached_content(cache_name);
            } else {
                builder = builder.system_instruction(
                    crate::brains::gemini::types::InteractionContent::from(instruction),
                );
            }
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
        let update_core_memory_tool = crate::brains::gemini::types::Tool::Function {
            declaration: crate::brains::gemini::types::FunctionDeclaration {
                name: "update_core_memory".to_string(),
                description: "Update the core MEMORY.md file with new permanent facts or preferences. Use this when the user explicitly states a preference, a rule, or a fact that should be remembered across all future conversations.".to_string(),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["append", "rewrite"] },
                        "content": { "type": "string" }
                    },
                    "required": ["action", "content"]
                })),
            },
        };
        builder = builder.tools(vec![ls_tool, update_core_memory_tool]);
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
                                        } else if fc.name == "update_core_memory" {
                                            ToolCallPayload::UpdateCoreMemory {
                                                action: fc.args.get("action").and_then(|v| v.as_str()).unwrap_or("append").to_string(),
                                                content: fc.args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
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
                        } else if fc.name == "update_core_memory" {
                            ToolCallPayload::UpdateCoreMemory {
                                action: fc
                                    .args
                                    .get("action")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("append")
                                    .to_string(),
                                content: fc
                                    .args
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
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
        MessagePart::Image { path } => {
            // Read the image file and convert to base64 inline data
            // For now, we will handle this inside the Conductor when reading files, but here we can just create a MediaPart
            let mime_type = match std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
            {
                Some("png") => "image/png",
                Some("jpeg") | Some("jpg") => "image/jpeg",
                Some("webp") => "image/webp",
                _ => "image/jpeg",
            }
            .to_string();

            if let Ok(data) = std::fs::read(path) {
                let base64_data =
                    std::sync::Arc::new(base64::engine::general_purpose::STANDARD.clone());
                use base64::Engine;
                let b64 = base64_data.encode(data);
                Some(InteractionPart::Image(
                    crate::brains::gemini::types::MediaPart {
                        uri: None,
                        data: Some(b64),
                        mime_type,
                    },
                ))
            } else {
                None
            }
        }
        MessagePart::ToolCall(tc) => {
            let (name, args) = match &tc.payload {
                ToolCallPayload::Ls { path } => {
                    let mut obj = serde_json::Map::new();
                    if let Some(p) = path {
                        obj.insert("path".to_string(), serde_json::Value::String(p.clone()));
                    }
                    ("ls".to_string(), serde_json::Value::Object(obj))
                }
                ToolCallPayload::UpdateCoreMemory { action, content } => {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "action".to_string(),
                        serde_json::Value::String(action.clone()),
                    );
                    obj.insert(
                        "content".to_string(),
                        serde_json::Value::String(content.clone()),
                    );
                    (
                        "update_core_memory".to_string(),
                        serde_json::Value::Object(obj),
                    )
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
                ToolResponsePayload::UpdateCoreMemory { result } => (
                    "update_core_memory".to_string(),
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::events::ConversationInput;
    use mockito::Server;
    use serde_json::json;

    #[tokio::test]
    async fn test_caching_fallback_and_creation() {
        let mut server = Server::new_async().await;
        let mock_cache = server
            .mock("POST", "/v1beta/cachedContents")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({ "name": "cachedContents/mock123", "model": "models/test" }).to_string(),
            )
            .create_async()
            .await;

        let mock_interaction = server
            .mock("POST", "/v1beta/interactions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "model": "gemini-3-flash-preview",
                    "status": "completed",
                    "outputs": [{ "type": "text", "text": "hello" }]
                })
                .to_string(),
            )
            .expect(2)
            .create_async()
            .await;

        let client = Client::new("key".into(), "model".into()).with_base_url(server.url());
        let engine = GeminiEngine::new(client);

        let mut context = TurnContext {
            input: ConversationInput::Text("hi".into()),
            previous_interaction_id: None,
            streaming: false,
            thinking_level: "low".into(),
            memory_enabled: true,
            system_instruction: Some("test instruction".into()),
        };

        // First turn should create cache
        let _ = engine.process_turn(context.clone()).await.unwrap();
        mock_cache.assert_async().await;

        // Second turn should reuse cache (cache mock not hit again, interaction mock hit again)
        let _ = engine.process_turn(context.clone()).await.unwrap();
        mock_interaction.assert_async().await;

        // Third turn with DIFFERENT instruction should create NEW cache
        let mock_cache_2 = server
            .mock("POST", "/v1beta/cachedContents")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({ "name": "cachedContents/mock456", "model": "models/test" }).to_string(),
            )
            .create_async()
            .await;

        context.system_instruction = Some("new test instruction".into());
        let _ = engine.process_turn(context.clone()).await.unwrap();
        mock_cache_2.assert_async().await;
    }
}
