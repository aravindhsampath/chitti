use async_trait::async_trait;
use tokio::sync::mpsc;
use anyhow::Result;
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent};

#[allow(dead_code)]
pub struct MockBridge {
    tx: mpsc::Sender<UserEvent>,
    pub system_events: mpsc::Sender<SystemEvent>, // To inspect events in tests
}

#[allow(dead_code)]
impl MockBridge {
    pub fn new() -> (Self, mpsc::Receiver<UserEvent>, mpsc::Receiver<SystemEvent>) {
        let (tx, rx) = mpsc::channel(100);
        let (stx, srx) = mpsc::channel(100);
        (Self { tx, system_events: stx }, rx, srx)
    }

    pub async fn simulate_user_message(&self, msg: String) -> Result<()> {
        self.tx.send(UserEvent::Input(msg)).await?;
        Ok(())
    }
}

#[async_trait]
impl CommBridge for MockBridge {
    async fn send(&self, event: SystemEvent) -> Result<()> {
        self.system_events.send(event).await?;
        Ok(())
    }
}
