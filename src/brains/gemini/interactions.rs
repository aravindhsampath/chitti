use crate::brains::gemini::client::Client;
use crate::brains::gemini::error::GeminiError;
use crate::brains::gemini::types::*;
use futures_util::{Stream, StreamExt, TryStreamExt};
use reqwest::{Method, Response};
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;
#[allow(unused_imports)]
use tracing::{debug, instrument, warn};

/// A builder for creating interaction requests.
pub struct InteractionRequestBuilder<'a> {
    client: &'a Client,
    request: InteractionRequest,
}

impl<'a> InteractionRequestBuilder<'a> {
    pub fn new(client: &'a Client, input: InteractionInput) -> Self {
        Self {
            client,
            request: InteractionRequest {
                model: Some(client.model.clone()),
                cached_content: None,
                agent: None,
                input,
                system_instruction: None,
                previous_interaction_id: None,
                tools: None,
                tool_choice: None,
                generation_config: None,
                safety_settings: None,
                store: Some(false), // Privacy first default
                background: None,
                stream: None,
            },
        }
    }

    #[allow(dead_code)]
    pub fn model(mut self, model: String) -> Self {
        self.request.model = Some(model);
        self
    }

    #[allow(dead_code)]
    pub fn cached_content(mut self, name: String) -> Self {
        self.request.cached_content = Some(name);
        self
    }

    #[allow(dead_code)]
    pub fn agent(mut self, agent: String) -> Self {
        self.request.agent = Some(agent);
        self.request.model = None; // Model and agent are mutually exclusive in API
        self
    }

    #[allow(dead_code)]
    pub fn system_instruction(mut self, instruction: InteractionContent) -> Self {
        self.request.system_instruction = Some(instruction);
        self
    }

    #[allow(dead_code)]
    pub fn previous_interaction_id(mut self, id: String) -> Self {
        self.request.previous_interaction_id = Some(id);
        self
    }

    #[allow(dead_code)]
    pub fn tools(mut self, tools: Vec<Tool>) -> Self {
        self.request.tools = Some(tools);
        self
    }

    #[allow(dead_code)]
    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.request.tool_choice = Some(choice);
        self
    }

    #[allow(dead_code)]
    pub fn generation_config(mut self, config: GenerationConfig) -> Self {
        self.request.generation_config = Some(config);
        self
    }

    #[allow(dead_code)]
    pub fn thinking_level(mut self, level: ThinkingLevel) -> Self {
        let mut config = self.request.generation_config.take().unwrap_or_default();
        config.thinking_level = Some(level);
        self.request.generation_config = Some(config);
        self
    }

    #[allow(dead_code)]
    pub fn store(mut self, store: bool) -> Self {
        self.request.store = Some(store);
        self
    }

    /// Sends the interaction request and returns the full response.
    #[allow(dead_code)]
    #[instrument(skip(self), fields(model = ?self.request.model))]
    pub async fn send(self) -> Result<InteractionResponse, GeminiError> {
        let response = self
            .client
            .request(Method::POST, "/v1beta/interactions")
            .json(&self.request)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            let message = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(msg) = json
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                {
                    msg.to_string()
                } else if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
                    msg.to_string()
                } else {
                    error_text.clone()
                }
            } else if error_text.starts_with("event: error") {
                let mut ext_msg = error_text.clone();
                for line in error_text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(msg) = json
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str())
                            {
                                ext_msg = msg.to_string();
                            }
                        }
                        break;
                    }
                }
                ext_msg
            } else {
                error_text.clone()
            };

            return Err(GeminiError::Api {
                code: status.to_string(),
                message,
            });
        }
        let text = response.text().await.map_err(GeminiError::Http)?;
        tracing::debug!(body = %text, "Received raw interaction response");
        let interaction_resp: InteractionResponse = serde_json::from_str(&text).map_err(|e| {
            tracing::error!(
                "Failed to parse interaction response: {} | Body: {}",
                e,
                text
            );
            GeminiError::Serde(e)
        })?;
        Ok(interaction_resp)
    }

    /// Starts a streaming interaction.
    #[instrument(skip(self), fields(model = ?self.request.model))]
    pub async fn stream(
        mut self,
    ) -> Result<impl Stream<Item = Result<InteractionEvent, GeminiError>>, GeminiError> {
        self.request.stream = Some(true);
        let response = self
            .client
            .request(Method::POST, "/v1beta/interactions")
            .json(&self.request)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            let message = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(msg) = json
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                {
                    msg.to_string()
                } else if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
                    msg.to_string()
                } else {
                    error_text.clone()
                }
            } else if error_text.starts_with("event: error") {
                let mut ext_msg = error_text.clone();
                for line in error_text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(msg) = json
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str())
                            {
                                ext_msg = msg.to_string();
                            }
                        }
                        break;
                    }
                }
                ext_msg
            } else {
                error_text.clone()
            };

            return Err(GeminiError::Api {
                code: status.to_string(),
                message,
            });
        }
        Ok(parse_sse_stream(response))
    }
}

fn parse_sse_stream(
    response: Response,
) -> impl Stream<Item = Result<InteractionEvent, GeminiError>> {
    let stream = response.bytes_stream().map_err(std::io::Error::other);

    let reader = StreamReader::new(stream);
    let codec = LinesCodec::new();
    let mut reader = FramedRead::new(reader, codec);
    async_stream::try_stream! {
        while let Some(line_res) = reader.next().await {
            let line = line_res?;
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    return;
                }
                match serde_json::from_str::<InteractionEvent>(data) {
                    Ok(event) => yield event,
                    Err(e) => {
                        warn!("Failed to parse SSE data: {} | Data: {}", e, data);
                    }
                }
            }
        }
    }
}

impl Client {
    #[instrument(skip(self))]
    /// Creates a new interaction builder with the given input.
    pub fn interaction(&self, input: InteractionInput) -> InteractionRequestBuilder<'_> {
        InteractionRequestBuilder::new(self, input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde_json::json;

    #[tokio::test]
    async fn test_interaction_send_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1beta/interactions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "model": "gemini-3-flash-preview",
                    "status": "completed",
                    "outputs": [{ "type": "text", "text": "Hello world" }]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = Client::new("test_key".to_string(), "gemini-3-flash-preview".to_string())
            .with_base_url(server.url());

        let response = client
            .interaction(InteractionInput::Text("Hi".to_string()))
            .send()
            .await
            .unwrap();

        assert_eq!(response.model, "gemini-3-flash-preview");
        assert_eq!(response.outputs.len(), 1);
        match &response.outputs[0] {
            InteractionOutput::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected text output"),
        }

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_interaction_send_error_json() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1beta/interactions")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "error": {
                        "code": "invalid_request",
                        "message": "Missing text in content"
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = Client::new("test_key".to_string(), "gemini-3-flash-preview".to_string())
            .with_base_url(server.url());

        let result = client
            .interaction(InteractionInput::Text("Hi".to_string()))
            .send()
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            GeminiError::Api { code, message } => {
                assert_eq!(code, "400 Bad Request");
                assert_eq!(message, "Missing text in content");
            }
            _ => panic!("Expected Api error"),
        }

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_interaction_send_error_sse() {
        let mut server = Server::new_async().await;
        let mock = server.mock("POST", "/v1beta/interactions")
            .with_status(400)
            .with_header("content-type", "text/event-stream")
            .with_body("event: error\ndata: {\"error\":{\"code\":\"invalid_request\",\"message\":\"Request contains an invalid argument.\"}}")
            .create_async().await;

        let client = Client::new("test_key".to_string(), "gemini-3-flash-preview".to_string())
            .with_base_url(server.url());

        let result = client
            .interaction(InteractionInput::Text("Hi".to_string()))
            .send()
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            GeminiError::Api { code, message } => {
                assert_eq!(code, "400 Bad Request");
                assert_eq!(message, "Request contains an invalid argument.");
            }
            _ => panic!("Expected Api error"),
        }

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_interaction_stream_success() {
        let mut server = Server::new_async().await;
        let mock = server.mock("POST", "/v1beta/interactions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body("data: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\"hello \"}}\n\ndata: {\"event_type\":\"content.delta\",\"delta\":{\"type\":\"text\",\"text\":\"world\"}}\n\ndata: [DONE]\n\n")
            .create_async().await;

        let client = Client::new("test_key".to_string(), "gemini-3-flash-preview".to_string())
            .with_base_url(server.url());

        let stream = client
            .interaction(InteractionInput::Text("Hi".to_string()))
            .stream()
            .await
            .unwrap();
        tokio::pin!(stream);

        let mut deltas = Vec::new();
        while let Some(Ok(InteractionEvent::ContentDelta { delta, .. })) = stream.next().await {
            if let InteractionOutput::Text { text } = delta {
                deltas.push(text);
            }
        }

        assert_eq!(deltas, vec!["hello ", "world"]);
        mock.assert_async().await;
    }
}
