use async_trait::async_trait;
use anyhow::Result;
use crate::conductor::events::SystemEvent;

pub mod tui;

#[async_trait]
pub trait CommBridge: Send + Sync {
    // Sends a message/update back to the user
    async fn send(&self, event: SystemEvent) -> Result<()>;
}
