use reqwest::{Client as HttpClient, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{info, debug, error};
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
}

impl Client {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            http_client: HttpClient::new(),
            api_key,
            model,
        }
    }

    pub async fn generate_content(&self, prompt: &str) -> Result<String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
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
