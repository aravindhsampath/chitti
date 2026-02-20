use async_trait::async_trait;
use serde_json::{Value, json};
use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;
use crate::tools::{ToolExecutor, ToolResult};
use crate::brains::gemini::types::FunctionDeclaration;

pub struct EditorTool;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct EditorArgs {
    operation: String,
    path: String,
    content: Option<String>,
}

#[async_trait]
impl ToolExecutor for EditorTool {
    fn name(&self) -> String {
        "file_editor".to_string()
    }

    fn definition(&self) -> FunctionDeclaration {
        FunctionDeclaration {
            name: self.name(),
            description: "Read, write, or list files and directories on the local system.".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["read", "write", "list"],
                        "description": "The operation to perform."
                    },
                    "path": {
                        "type": "string",
                        "description": "The path to the file or directory."
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write (required for 'write' operation)."
                    }
                },
                "required": ["operation", "path"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let editor_args: EditorArgs = serde_json::from_value(args)?;
        let path = PathBuf::from(&editor_args.path);

        let (output, is_error) = match editor_args.operation.as_str() {
            "read" => {
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => (json!({ "content": content }), false),
                    Err(e) => (json!({ "error": format!("Failed to read file: {}", e) }), true),
                }
            }
            "write" => {
                if let Some(content) = editor_args.content {
                    match tokio::fs::write(&path, content).await {
                        Ok(_) => (json!({ "status": "success" }), false),
                        Err(e) => (json!({ "error": format!("Failed to write file: {}", e) }), true),
                    }
                } else {
                    (json!({ "error": "Missing 'content' for write operation" }), true)
                }
            }
            "list" => {
                match tokio::fs::read_dir(&path).await {
                    Ok(mut entries) => {
                        let mut files = Vec::new();
                        while let Some(entry) = entries.next_entry().await? {
                            files.push(entry.file_name().to_string_lossy().to_string());
                        }
                        (json!({ "entries": files }), false)
                    }
                    Err(e) => (json!({ "error": format!("Failed to list directory: {}", e) }), true),
                }
            }
            _ => (json!({ "error": format!("Unknown operation: {}", editor_args.operation) }), true),
        };

        Ok(ToolResult {
            output,
            is_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_editor_list() -> Result<()> {
        let tool = EditorTool;
        let args = json!({
            "operation": "list",
            "path": "."
        });
        let result = tool.execute(args).await?;
        assert!(!result.is_error);
        Ok(())
    }

    #[tokio::test]
    async fn test_editor_write_read() -> Result<()> {
        let tool = EditorTool;
        let test_file = "tests/editor_test.txt";
        let test_content = "Hello Editor!";

        // 1. Write
        let write_args = json!({
            "operation": "write",
            "path": test_file,
            "content": test_content
        });
        let write_res = tool.execute(write_args).await?;
        assert!(!write_res.is_error);

        // 2. Read
        let read_args = json!({
            "operation": "read",
            "path": test_file
        });
        let read_res = tool.execute(read_args).await?;
        assert!(!read_res.is_error);
        assert_eq!(read_res.output["content"], test_content);

        // Cleanup
        let _ = tokio::fs::remove_file(test_file).await;
        Ok(())
    }
}
