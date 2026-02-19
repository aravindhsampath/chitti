use anyhow::{Context, Result};
use reqwest::Method;
use crate::gemini::client::Client;
use crate::gemini::types::*;

impl Client {
    /// Creates a batch for processing multiple requests.
    pub async fn create_batch(&self, display_name: String, file_name: String) -> Result<Operation> {
        let path = format!("/v1beta/models/{}:batchGenerateContent", self.model);
        let request = BatchRequest {
            display_name,
            input_config: BatchInputConfig::FileName { file_name },
        };

        let response = self.request(Method::POST, &path)
            .json(&request)
            .send()
            .await
            .context("Failed to create batch")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Batch API create error: {}", response.status()));
        }

        Ok(response.json().await?)
    }

    /// Gets the status of a batch operation.
    pub async fn get_batch_operation(&self, name: &str) -> Result<Operation> {
        let path = if name.starts_with("batches/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/batches/{}", name)
        };

        let response = self.request(Method::GET, &path)
            .send()
            .await
            .context("Failed to get batch status")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Batch API get error: {}", response.status()));
        }

        Ok(response.json().await?)
    }
}
