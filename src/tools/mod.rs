use async_trait::async_trait;
use serde_json::Value;
use anyhow::Result;
use std::collections::HashMap;
use crate::brains::gemini::types::FunctionDeclaration;

pub mod bash;
pub mod editor;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: Value,
    pub is_error: bool,
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn name(&self) -> String;
    fn definition(&self) -> FunctionDeclaration;
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn ToolExecutor>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn ToolExecutor>) {
        self.tools.insert(tool.name(), tool);
    }

    pub fn get_definitions(&self) -> Vec<crate::brains::gemini::types::Tool> {
        self.tools.values().map(|t| {
            crate::brains::gemini::types::Tool::Function {
                declaration: t.definition()
            }
        }).collect()
    }

    pub async fn execute(&self, name: &str, args: serde_json::Value) -> Result<ToolResult> {
        let tool = self.tools.get(name).ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;
        tool.execute(args).await
    }
}
