use reqwest::{Client as HttpClient, Method, RequestBuilder};
use tracing::debug;

/// The base Gemini API client.
#[derive(Clone)]
pub struct Client {
    pub(crate) http_client: HttpClient,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Client {
    /// Creates a new Gemini API client.
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            http_client: HttpClient::new(),
            api_key,
            model,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
        }
    }

    /// Sets a custom model for the client.
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Sets a custom base URL (useful for testing/mocking).
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Builds a request with the necessary headers and API key.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        debug!("Building request: {} {}", method, url);
        
        self.http_client
            .request(method, &url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
    }
}
