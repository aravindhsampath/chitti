use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use anyhow::Result;
use crate::brains::gemini::types::InteractionTurn;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Session {
    pub interaction_id: Option<String>,
    pub turns: Vec<InteractionTurn>,
    pub model: String,
    pub thinking_level: String,
    pub memory_enabled: bool,
    pub dev_mode: bool,
}

impl Session {
    pub fn new(model: String, dev_mode: bool) -> Self {
        Self {
            interaction_id: None,
            turns: Vec::new(),
            model,
            thinking_level: "high".to_string(),
            memory_enabled: true,
            dev_mode,
        }
    }

    pub async fn load(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow::anyhow!("Session file not found: {:?}", path));
        }
        let data = tokio::fs::read_to_string(path).await?;
        let session: Session = serde_json::from_str(&data)?;
        Ok(session)
    }

    pub async fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, data).await?;
        Ok(())
    }
}
