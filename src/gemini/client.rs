use reqwest::{Client as HttpClient, Method, RequestBuilder as ReqwestRequestBuilder, Response};
use tracing::{debug, instrument, warn};
use crate::gemini::error::GeminiError;
use std::time::Duration;
use tokio::time::sleep;

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
    #[instrument(skip(self))]
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        debug!("Building request: {} {}", method, url);
        
        let request_id = uuid::Uuid::new_v4();
        let inner = self.http_client
            .request(method.clone(), &url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .header("X-Request-ID", request_id.to_string());
        RequestBuilder { 
            inner, 
            method: method.to_string(), 
            url,
            request_id,
        }
    }
}

/// A wrapper around reqwest::RequestBuilder to add retry logic and tracing.
pub struct RequestBuilder {
    inner: ReqwestRequestBuilder,
    method: String,
    url: String,
    request_id: uuid::Uuid,
}

impl RequestBuilder {
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        reqwest::header::HeaderName: TryFrom<K>,
        <reqwest::header::HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        reqwest::header::HeaderValue: TryFrom<V>,
        <reqwest::header::HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.inner = self.inner.header(key, value);
        self
    }

    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        self.inner = self.inner.query(query);
        self
    }

    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> Self {
        self.inner = self.inner.json(json);
        self
    }
    
    pub fn body<T: Into<reqwest::Body>>(mut self, body: T) -> Self {
        self.inner = self.inner.body(body);
        self
    }

    #[instrument(
        skip(self), 
        fields(
            request_id = %self.request_id,
            method = %self.method, 
            url = %self.url
        )
    )]
    pub async fn send(self) -> Result<Response, GeminiError> {
        let mut attempt = 1;
        let max_retries = 3;
        let mut backoff = Duration::from_secs(1);
        
        // We use Option to handle ownership of the source builder across retry loops
        let mut source = Some(self.inner);

        loop {
            // Determine if we can potentially retry after this attempt
            let is_last_attempt = attempt > max_retries;
            
            // prepare the request to send
            let request_to_send = if is_last_attempt {
                // Last attempt: consume the source
                source.take().ok_or_else(|| GeminiError::Other("Request builder exhausted".to_string()))?
            } else {
                // Not last attempt: try to clone
                // We need to access source without consuming it yet
                match source.as_ref().and_then(|s| s.try_clone()) {
                    Some(cloned) => cloned,
                    None => {
                        warn!("Request body is not cloneable, retries disabled for this request");
                        // Can't clone, so we must consume source
                        source.take().ok_or_else(|| GeminiError::Other("Request builder exhausted".to_string()))?
                    }
                }
            };

            debug!(attempt, "Sending request");
            match request_to_send.send().await {
                Ok(response) => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    
                    if let Some(req_id) = headers.get("x-goog-request-id") {
                        debug!(request_id = ?req_id, "Received response from Gemini");
                    }

                    if status.is_success() {
                        return Ok(response);
                    }

                    // Check for retryable status codes
                    // We can only retry if we still have the source (i.e., we cloned it)
                    if source.is_some() && (status == 429 || status == 500 || status == 503) {
                         warn!(
                            attempt,
                            status = %status,
                            "Request failed with retryable status, retrying in {:?}...",
                            backoff
                        );
                        sleep(backoff).await;
                        attempt += 1;
                        backoff *= 2;
                        continue;
                    }
                    
                    return Ok(response);
                }
                Err(e) => {
                    // We can only retry if we still have the source
                    if source.is_some() {
                        warn!(
                            attempt,
                            error = %e,
                            "Request failed with network error, retrying in {:?}...",
                            backoff
                        );
                        sleep(backoff).await;
                        attempt += 1;
                        backoff *= 2;
                        continue;
                    }
                    
                    return Err(GeminiError::Http(e));
                }
            }
        }
    }
}
