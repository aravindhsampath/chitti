use crate::conductor::events::SystemEvent;
use anyhow::Result;
use async_trait::async_trait;

pub mod mock;
pub mod tui;

#[async_trait]
pub trait CommBridge: Send + Sync {
    // Sends a message/update back to the user
    async fn send(&self, event: SystemEvent) -> Result<()>;
}
