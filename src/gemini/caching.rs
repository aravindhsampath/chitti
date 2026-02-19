use reqwest::Method;
use tracing::instrument;
use crate::gemini::client::Client;
use crate::gemini::types::*;
use crate::gemini::error::{GeminiError, Result};

impl Client {
    /// Creates a new cached content resource.
    #[instrument(skip(self, cached_content), fields(model = %self.model))]
    pub async fn create_cached_content(&self, cached_content: CachedContent) -> Result<CachedContent> {
        let response = self.request(Method::POST, "/v1beta/cachedContents")
            .json(&cached_content)
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

    /// Gets metadata for a cached content.
    #[instrument(skip(self))]
    pub async fn get_cached_content(&self, name: &str) -> Result<CachedContent> {
        let path = if name.starts_with("cachedContents/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/cachedContents/{}", name)
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

    /// Deletes a cached content.
    #[instrument(skip(self))]
    pub async fn delete_cached_content(&self, name: &str) -> Result<()> {
        let path = if name.starts_with("cachedContents/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/cachedContents/{}", name)
        };

        let response = self.request(Method::DELETE, &path)
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

        Ok(())
    }
}
