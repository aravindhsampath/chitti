use reqwest::Method;
use std::path::Path;
use crate::gemini::client::Client;
use crate::gemini::types::*;
use crate::gemini::error::{GeminiError, Result};
impl Client {
    /// Uploads a file to the Gemini File API.
    pub async fn upload_file<P: AsRef<Path>>(&self, path: P, display_name: Option<String>) -> Result<File> {
        let path = path.as_ref();
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let mime_type = mime_guess::from_path(path)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string();

        let file_bytes = tokio::fs::read(path).await?;
        // 1. Initial metadata request
        let metadata = serde_json::json!({
            "file": {
                "display_name": display_name.unwrap_or(file_name),
            }
        });

        let response = self.http_client
            .request(Method::POST, "https://generativelanguage.googleapis.com/upload/v1beta/files")
            .header("x-goog-api-key", &self.api_key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header("X-Goog-Upload-Header-Content-Length", file_bytes.len())
            .header("X-Goog-Upload-Header-Content-Type", &mime_type)
            .header("Content-Type", "application/json")
            .json(&metadata)
            .send()
            .await?;

        if !response.status().is_success() {
            let code = response.status().as_str().to_string();
            let message = response.text().await.unwrap_or_default();
            return Err(GeminiError::Api { code, message });
        }
        let upload_url = response.headers()
            .get("x-goog-upload-url")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| GeminiError::Other("Missing x-goog-upload-url header".to_string()))?
            .to_string();
        // 2. Upload actual bytes
        let response = self.http_client
            .request(Method::POST, &upload_url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Length", file_bytes.len())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(file_bytes)
            .send()
            .await?;

        if !response.status().is_success() {
            let code = response.status().as_str().to_string();
            let message = response.text().await.unwrap_or_default();
            return Err(GeminiError::Api { code, message });
        }
        let result: serde_json::Value = response.json().await?;
        let file: File = serde_json::from_value(result["file"].clone())?;
        Ok(file)
    }
    /// Gets metadata for a file.
    pub async fn get_file(&self, name: &str) -> Result<File> {
        let path = if name.starts_with("files/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/files/{}", name)
        };
        let response = self.request(Method::GET, &path)
            .send()
            .await?;

        if !response.status().is_success() {
            let code = response.status().as_str().to_string();
            let message = response.text().await.unwrap_or_default();
            return Err(GeminiError::Api { code, message });
        }

        Ok(response.json().await?)
    }
    /// Lists files owned by the project.
    pub async fn list_files(&self, page_size: Option<u32>, page_token: Option<String>) -> Result<ListFilesResponse> {
        let mut query = vec![];
        if let Some(ps) = page_size {
            query.push(("pageSize", ps.to_string()));
        }
        if let Some(pt) = page_token {
            query.push(("pageToken", pt));
        }
        let response = self.request(Method::GET, "/v1beta/files")
            .query(&query)
            .send()
            .await?;

        if !response.status().is_success() {
            let code = response.status().as_str().to_string();
            let message = response.text().await.unwrap_or_default();
            return Err(GeminiError::Api { code, message });
        }

        Ok(response.json().await?)
    }
    /// Deletes a file.
    pub async fn delete_file(&self, name: &str) -> Result<()> {
        let path = if name.starts_with("files/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/files/{}", name)
        };
        let response = self.request(Method::DELETE, &path)
            .send()
            .await?;

        if !response.status().is_success() {
            let code = response.status().as_str().to_string();
            let message = response.text().await.unwrap_or_default();
            return Err(GeminiError::Api { code, message });
        }
        Ok(())
    }
}