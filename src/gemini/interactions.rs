use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::{Method, Response};
use tracing::warn;
use crate::gemini::client::Client;
use crate::gemini::types::*;

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

    pub fn model(mut self, model: String) -> Self {
        self.request.model = Some(model);
        self
    }

    pub fn agent(mut self, agent: String) -> Self {
        self.request.agent = Some(agent);
        self.request.model = None; // Model and agent are mutually exclusive in API
        self
    }

    pub fn system_instruction(mut self, instruction: Content) -> Self {
        self.request.system_instruction = Some(instruction);
        self
    }

    pub fn previous_interaction_id(mut self, id: String) -> Self {
        self.request.previous_interaction_id = Some(id);
        self
    }

    pub fn tools(mut self, tools: Vec<Tool>) -> Self {
        self.request.tools = Some(tools);
        self
    }


    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.request.tool_choice = Some(choice);
        self
    }

    pub fn generation_config(mut self, config: GenerationConfig) -> Self {
        self.request.generation_config = Some(config);
        self
    }

    pub fn thinking_level(mut self, level: ThinkingLevel) -> Self {
        let mut config = self.request.generation_config.take().unwrap_or_default();
        config.thinking_config = Some(ThinkingConfig { thinking_level: level });
        self.request.generation_config = Some(config);
        self
    }

    pub fn store(mut self, store: bool) -> Self {
        self.request.store = Some(store);
        self
    }

    /// Sends the interaction request and returns the full response.
    pub async fn send(self) -> Result<InteractionResponse> {
        let response = self.client
            .request(Method::POST, "/v1beta/interactions")
            .json(&self.request)
            .send()
            .await
            .context("Failed to send interaction request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Interaction API error ({}): {}", status, error_text));
        }

        let interaction_resp: InteractionResponse = response.json().await
            .context("Failed to parse interaction response")?;
        
        Ok(interaction_resp)
    }

    /// Starts a streaming interaction.
    pub async fn stream(mut self) -> Result<impl futures_util::Stream<Item = Result<InteractionEvent>>> {
        self.request.stream = Some(true);
        
        let response = self.client
            .request(Method::POST, "/v1beta/interactions")
            .json(&self.request)
            .send()
            .await
            .context("Failed to start interaction stream")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Interaction Stream API error ({}): {}", status, error_text));
        }

        Ok(parse_sse_stream(response))
    }
}

fn parse_sse_stream(response: Response) -> impl futures_util::Stream<Item = Result<InteractionEvent>> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    async_stream::try_stream! {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error reading stream chunk")?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer.drain(..line_end + 1).collect::<String>();
                let line = line.trim();

                if line.starts_with("data: ") {
                    let data = &line["data: ".len()..];
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
}

impl Client {
    /// Creates a new interaction builder with the given input.
    pub fn interaction(&self, input: InteractionInput) -> InteractionRequestBuilder<'_> {
        InteractionRequestBuilder::new(self, input)
    }
}
