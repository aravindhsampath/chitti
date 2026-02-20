use async_trait::async_trait;
use futures_util::stream::BoxStream;
use crate::conductor::events::{BrainEvent, TurnContext};
use anyhow::Result;

pub mod gemini;

#[async_trait]
pub trait BrainEngine: Send + Sync {
    async fn process_turn(&self, context: TurnContext) -> Result<BoxStream<'static, Result<BrainEvent>>>;
}
