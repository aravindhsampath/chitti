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
}

impl MemoryManager {
    pub fn new(db: Arc<Db>, rx: mpsc::Receiver<TriggerMemoryCheck>) -> Self {
        Self { db, rx }
    }

    pub async fn run(mut self) {
        while let Some(trigger) = self.rx.recv().await {
            tracing::info!("MemoryManager triggered for topic: {}", trigger.topic_id);
            // Tidal Summarization Logic (Step 8) will go here
        }
    }
}
