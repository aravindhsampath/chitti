use reqwest::Method;
use tracing::instrument;
use crate::brains::gemini::client::Client;
use crate::brains::gemini::types::*;
use crate::brains::gemini::error::{GeminiError, Result};

impl Client {
    /// Creates a batch for processing multiple requests.
    #[instrument(skip(self), fields(model = %self.model))]
    #[allow(dead_code)]
    pub async fn create_batch(&self, display_name: String, file_name: String) -> Result<Operation> {
        let path = format!("/v1beta/models/{}:batchGenerateContent", self.model);
        let request = BatchRequest {
            display_name,
            input_config: BatchInputConfig { file_name },
        };

        let response = self.request(Method::POST, &path)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            
            // Try to parse ApiError
            let message = if let Ok(api_error) = serde_json::from_str::<ApiError>(&text) {
                api_error.message
            } else {
                text
            };

            return Err(GeminiError::Api {
                code: status.to_string(),
                message,
            });
        }

        Ok(response.json().await?)
    }

    /// Gets the status of a batch operation.
    #[instrument(skip(self))]
    #[allow(dead_code)]
    pub async fn get_batch_operation(&self, name: &str) -> Result<Operation> {
        let path = if name.starts_with("batches/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/batches/{}", name)
        };

        let response = self.request(Method::GET, &path)
            .send()
            .await?;

        if !response.status().is_success() {
             let status = response.status();
            let text = response.text().await.unwrap_or_default();
            
            // Try to parse ApiError
            let message = if let Ok(api_error) = serde_json::from_str::<ApiError>(&text) {
                api_error.message
            } else {
                text
            };

            return Err(GeminiError::Api {
                code: status.to_string(),
                message,
            });
        }

        Ok(response.json().await?)
    }
}
