use anyhow::{Context, Result};
use reqwest::Method;
use crate::gemini::client::Client;
use crate::gemini::types::*;

impl Client {
    /// Creates a new cached content resource.
    pub async fn create_cached_content(&self, cached_content: CachedContent) -> Result<CachedContent> {
        let response = self.request(Method::POST, "/v1beta/cachedContents")
            .json(&cached_content)
            .send()
            .await
            .context("Failed to create cached content")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Caching API create error: {}", response.status()));
        }

        Ok(response.json().await?)
    }

    /// Gets metadata for a cached content.
    pub async fn get_cached_content(&self, name: &str) -> Result<CachedContent> {
        let path = if name.starts_with("cachedContents/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/cachedContents/{}", name)
        };

        let response = self.request(Method::GET, &path)
            .send()
            .await
            .context("Failed to get cached content")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Caching API get error: {}", response.status()));
        }

        Ok(response.json().await?)
    }

    /// Deletes a cached content.
    pub async fn delete_cached_content(&self, name: &str) -> Result<()> {
        let path = if name.starts_with("cachedContents/") {
            format!("/v1beta/{}", name)
        } else {
            format!("/v1beta/cachedContents/{}", name)
        };

        let response = self.request(Method::DELETE, &path)
            .send()
            .await
            .context("Failed to delete cached content")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Caching API delete error: {}", response.status()));
        }

        Ok(())
    }
}
