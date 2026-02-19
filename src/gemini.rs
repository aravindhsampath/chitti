use reqwest::{Client as HttpClient, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};
use anyhow::{Result, anyhow};

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
}

#[derive(Serialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct Part {
    text: String,
}

#[derive(Deserialize, Debug)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize, Debug)]
struct Candidate {
    content: ContentResponse,
}

#[derive(Deserialize, Debug)]
struct ContentResponse {
    parts: Vec<PartResponse>,
}

#[derive(Deserialize, Debug)]
struct PartResponse {
    text: String,
}

pub struct Client {
    http_client: HttpClient,
    api_key: String,
    model: String,
    base_url: String,
}

impl Client {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            http_client: HttpClient::new(),
            api_key,
            model,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(api_key: String, model: String, base_url: String) -> Self {
        Self {
            http_client: HttpClient::new(),
            api_key,
            model,
            base_url,
        }
    }

    pub async fn generate_content(&self, prompt: &str) -> Result<String> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let request_body = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
        };

        debug!("Sending request to Gemini: {}", url);
        
        let response = self.http_client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if status != StatusCode::OK {
            let error_text = response.text().await?;
            error!("Gemini API error ({}): {}", status, error_text);
            return Err(anyhow!("Gemini API error ({}): {}", status, error_text));
        }

        let gemini_resp: GeminiResponse = response.json().await?;
        debug!("Received response: {:?}", gemini_resp);

        if let Some(candidates) = gemini_resp.candidates {
            if let Some(candidate) = candidates.get(0) {
                if let Some(part) = candidate.content.parts.get(0) {
                    return Ok(part.text.clone());
                }
            }
        }

        Err(anyhow!("Gemini API returned an empty or invalid response structure"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[tokio::test]
    async fn test_generate_content_success() {
        let mut server = Server::new_async().await;
        // Mock the exact path structure used in Client::generate_content
        // URL format: {base_url}/v1beta/models/{model}:generateContent?key={key}
        let path = "/v1beta/models/test-model:generateContent";
        
        let mock = server.mock("POST", path)
            .match_query(mockito::Matcher::Regex("key=test-key".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "candidates": [
                    {
                        "content": {
                            "parts": [
                                {
                                    "text": "Hello from Gemini!"
                                }
                            ]
                        }
                    }
                ]
            }"#)
            .create_async().await;

        let client = Client::with_base_url(
            "test-key".to_string(),
            "test-model".to_string(),
            server.url(),
        );

        let response = client.generate_content("Hi").await.unwrap();
        assert_eq!(response, "Hello from Gemini!");
        
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_generate_content_api_error() {
        let mut server = Server::new_async().await;
        let path = "/v1beta/models/test-model:generateContent";
        
        let mock = server.mock("POST", path)
            .match_query(mockito::Matcher::Regex("key=test-key".into()))
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async().await;

        let client = Client::with_base_url(
            "test-key".to_string(),
            "test-model".to_string(),
            server.url(),
        );

        let result = client.generate_content("Hi").await;
        assert!(result.is_err());
        
        mock.assert_async().await;
    }
}