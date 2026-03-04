use crate::brains::gemini::adapter::GeminiEngine;
use crate::brains::gemini::Client;
use crate::brains::BrainEngine;
use crate::conductor::events::{ConversationInput, TurnContext};
use crate::memory::db::Db;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct TriggerMemoryCheck {
    pub topic_id: String,
}

#[allow(dead_code)]
pub struct MemoryManager {
    db: Arc<Db>,
    rx: mpsc::Receiver<TriggerMemoryCheck>,
    api_key: String,
}

impl MemoryManager {
    pub fn new(db: Arc<Db>, rx: mpsc::Receiver<TriggerMemoryCheck>, api_key: String) -> Self {
        Self { db, rx, api_key }
    }

    pub async fn run(mut self) {
        let n = 10;
        let k = 5;
        let threshold = n + k;

        while let Some(trigger) = self.rx.recv().await {
            tracing::info!("MemoryManager triggered for topic: {}", trigger.topic_id);

            // Fetch topic state
            if let Ok(Some(topic_state)) = self.db.fetch_topic_state(&trigger.topic_id).await {
                // Count unsummarized logs
                if let Ok(count) = self
                    .db
                    .count_unsummarized_logs(&trigger.topic_id, topic_state.last_summarized_log_id)
                    .await
                {
                    if count >= threshold {
                        // We need to summarize the oldest K logs
                        if let Ok(logs_to_summarize) = self
                            .db
                            .fetch_oldest_unsummarized_logs(
                                &trigger.topic_id,
                                topic_state.last_summarized_log_id,
                                k,
                            )
                            .await
                        {
                            if logs_to_summarize.is_empty() {
                                continue;
                            }

                            let mut prompt = String::new();
                            prompt.push_str("Merge the following recent messages into the existing topic summary to create a new, concise summary. Only return the new summary text. If the user states a new permanent fact, preference, or rule, use the update_core_memory tool to save it.\n\n");
                            prompt.push_str("EXISTING SUMMARY:\n");
                            prompt.push_str(&topic_state.summary);
                            prompt.push_str("\n\nRECENT MESSAGES:\n");
                            for log in &logs_to_summarize {
                                prompt.push_str(&format!("{}: {}\n", log.role, log.content));
                            }

                            let client = Client::new(
                                self.api_key.clone(),
                                "gemini-3-flash-preview".to_string(),
                            );
                            let engine = GeminiEngine::new(client);

                            let context = TurnContext {
                                input: ConversationInput::Text(prompt),
                                previous_interaction_id: None,
                                streaming: false,
                                thinking_level: "low".to_string(),
                                memory_enabled: false,
                                system_instruction: None,
                            };

                            if let Ok(mut stream) = engine.process_turn(context).await {
                                use futures_util::StreamExt;
                                let mut new_summary = String::new();
                                while let Some(res) = stream.next().await {
                                    if let Ok(crate::conductor::events::BrainEvent::TextDelta(
                                        text,
                                    )) = res
                                    {
                                        new_summary.push_str(&text);
                                    } else if let Ok(
                                        crate::conductor::events::BrainEvent::ToolCall(tool_call),
                                    ) = res
                                    {
                                        if let crate::conductor::events::ToolCallPayload::UpdateCoreMemory { action, content } = tool_call.payload {
                                            tracing::info!("MemoryManager: Updating core memory (action: {})", action);
                                            let mem_path = std::path::PathBuf::from("MEMORY.md");
                                            if action == "append" {
                                                if let Ok(mut current) = std::fs::read_to_string(&mem_path) {
                                                    current.push('\n');
                                                    current.push_str(&content);
                                                    let _ = std::fs::write(&mem_path, current);
                                                } else {
                                                    let _ = std::fs::write(&mem_path, content);
                                                }
                                            } else if action == "rewrite" {
                                                let _ = std::fs::write(&mem_path, content);
                                            }
                                        }
                                    }
                                }

                                if !new_summary.is_empty() {
                                    let new_last_id = logs_to_summarize.last().unwrap().id;
                                    let _ = self
                                        .db
                                        .update_topic_summary(
                                            &trigger.topic_id,
                                            new_summary.trim(),
                                            new_last_id,
                                        )
                                        .await;
                                    tracing::info!(
                                        "Updated topic summary for {} up to log ID {}",
                                        trigger.topic_id,
                                        new_last_id
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
