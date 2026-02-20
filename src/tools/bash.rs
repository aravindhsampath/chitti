use async_trait::async_trait;
use serde_json::{Value, json};
use serde::Deserialize;
use anyhow::Result;
use std::collections::HashMap;
use tokio::process::Command;
use crate::tools::{ToolExecutor, ToolResult};
use crate::brains::gemini::types::FunctionDeclaration;

pub struct BashTool;

#[derive(Deserialize)]
struct BashArgs {
    command: String,
}

#[async_trait]
impl ToolExecutor for BashTool {
    fn name(&self) -> String {
        "execute_bash".to_string()
    }

    fn definition(&self) -> FunctionDeclaration {
        FunctionDeclaration {
            name: self.name(),
            description: "Execute a bash command on the local macOS system to read files, search code, or manage system state.".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The full bash command to execute (e.g., 'ls -la' or 'rg search_term')."
                    }
                },
                "required": ["command"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let bash_args: BashArgs = serde_json::from_value(args)?;
        let command_str = &bash_args.command;

        let output = Command::new("bash")
            .arg("-c")
            .arg(command_str)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let result_json = json!({
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": output.status.code().unwrap_or(-1)
        });

        Ok(ToolResult {
            output: result_json,
            is_error: !output.status.success(),
        })
    }
}
