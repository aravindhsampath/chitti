use async_trait::async_trait;
use serde_json::{Value, json};
use anyhow::Result;
use serde::Deserialize;
use crate::tools::{ToolExecutor, ToolResult};
use crate::brains::gemini::types::FunctionDeclaration;

pub struct WebTool;

#[derive(Deserialize)]
struct WebArgs {
    url: String,
}

#[async_trait]
impl ToolExecutor for WebTool {
    fn name(&self) -> String {
        "fetch_web".to_string()
    }

    fn definition(&self) -> FunctionDeclaration {
        FunctionDeclaration {
            name: self.name(),
            description: "Fetch a web page and return its content (HTML or JSON).".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch."
                    }
                },
                "required": ["url"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let web_args: WebArgs = serde_json::from_value(args)?;
        let client = reqwest::Client::new();
        
        match client.get(&web_args.url).send().await {
            Ok(resp) => {
                let status = resp.status();
                match resp.text().await {
                    Ok(body) => {
                        Ok(ToolResult {
                            output: json!({
                                "status": status.as_u16(),
                                "body": body
                            }),
                            is_error: !status.is_success(),
                        })
                    }
                    Err(e) => {
                        Ok(ToolResult {
                            output: json!({ "error": format!("Failed to read response body: {}", e) }),
                            is_error: true,
                        })
                    }
                }
            }
            Err(e) => {
                Ok(ToolResult {
                    output: json!({ "error": format!("Failed to fetch URL: {}", e) }),
                    is_error: true,
                })
            }
        }
    }
}
