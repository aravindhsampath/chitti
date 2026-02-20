use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The role of the content creator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Model,
    Tool,
}

// --- Legacy / generateContent API Structs (Used by Caching/Batch) ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<Blob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    pub mime_type: String,
    pub file_uri: String,
}

// --- Interactions API Structs ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionPart {
    Text { text: String },
    Thought {
        #[serde(default)]
        signature: String,
        #[serde(skip_serializing_if = "String::is_empty", default)]
        summary: String,
    },
    Image(MediaPart),
    Audio(MediaPart),
    Video(MediaPart),
    Document(MediaPart),
    FunctionCall(FunctionCall),
    #[serde(rename = "function_result")]
    FunctionResponse(FunctionResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InteractionContent(pub Vec<InteractionPart>);

impl From<String> for InteractionContent {
    fn from(text: String) -> Self {
        Self(vec![InteractionPart::Text { text }])
    }
}

impl From<Vec<InteractionPart>> for InteractionContent {
    fn from(parts: Vec<InteractionPart>) -> Self {
        Self(parts)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InteractionInput {
    Text(String),
    Parts(Vec<InteractionPart>),
    Turns(Vec<InteractionTurn>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InteractionTurn {
    pub role: Role,
    pub content: InteractionContent,
}

// --- Shared Structs ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MediaPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "arguments")]
    pub args: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionResponse {
    #[serde(rename = "call_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "result")]
    pub response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    GoogleSearch,
    CodeExecution,
    UrlContext,
    Function {
        #[serde(flatten)]
        declaration: FunctionDeclaration,
    },
    ComputerUse {
        environment: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        excluded_predefined_functions: Option<Vec<String>>,
    },
    McpServer {
        name: String,
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Any,
    Function { name: String },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_modalities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InteractionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub input: InteractionInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<InteractionContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_interaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub struct InteractionResponse {
    pub id: Option<String>,
    pub model: String,
    pub status: String,
    #[serde(default)]
    pub outputs: Vec<InteractionOutput>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// An output from an interaction.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionOutput {
    Text { 
        #[serde(default)]
        text: String 
    },
    Thought { 
        #[serde(default)]
        signature: String,
        #[serde(default)]
        summary: String 
    },
    #[serde(rename = "thought_signature")]
    ThoughtSignature { 
        #[serde(default)]
        signature: String 
    },
    Image(MediaPart),
    Audio(MediaPart),
    Video(MediaPart),
    Document(MediaPart),
    FunctionCall(FunctionCall),
    FunctionResponse(FunctionResponse),
    SearchTool(serde_json::Value),
    GoogleSearchCall(serde_json::Value),
    GoogleSearchResult(serde_json::Value),
    ContentDelta { 
        #[serde(default)]
        text: String, 
        thought: Option<bool> 
    },
    ThoughtSummary {
        #[serde(default)]
        summary: String,
        #[serde(default)]
        signature: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentStartInfo {
    #[serde(rename = "type")]
    pub content_type: String,
}

/// Events yielded during a streaming interaction.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum InteractionEvent {
    #[serde(rename = "interaction.start")]
    InteractionStart { interaction: InteractionResponse },
    #[serde(rename = "interaction.status_update")]
    StatusUpdate { status: String },
    #[serde(rename = "content.start")]
    ContentStart { 
        index: u32,
        content: ContentStartInfo,
    },
    #[serde(rename = "content.delta")]
    ContentDelta { delta: InteractionOutput, index: Option<u32> },
    #[serde(rename = "interaction.complete")]
    InteractionComplete { interaction: InteractionResponse },
    #[serde(other)]
    Other,
}

/// Structured API error.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Safety setting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub struct SafetySetting {
    pub category: SafetyCategory,
    pub threshold: SafetyThreshold,
}

/// Safety categories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyCategory {
    HateSpeech,
    SexuallyExplicit,
    Harassment,
    DangerousContent,
    CivicIntegrity,
}

/// Safety thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyThreshold {
    BlockNone,
    BlockOnlyHigh,
    BlockMediumAndAbove,
    BlockLowAndAbove,
}

/// File metadata.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub name: String,
    pub display_name: Option<String>,
    pub mime_type: String,
    pub size_bytes: String,
    pub create_time: String,
    pub update_time: String,
    pub expiration_time: Option<String>,
    pub sha256_hash: String,
    pub uri: String,
    pub download_uri: Option<String>,
    pub state: FileState,
    pub source: Option<FileSource>,
    pub error: Option<serde_json::Value>,
}

/// File state.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileState {
    StateUnspecified,
    Processing,
    Active,
    Failed,
}

/// File source.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileSource {
    SourceUnspecified,
    Uploaded,
    Generated,
    Registered,
}

/// List files response.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesResponse {
    pub files: Vec<File>,
    pub next_page_token: Option<String>,
}

/// Batch request.
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    pub display_name: String,
    pub input_config: BatchInputConfig,
}

/// Batch input config.
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct BatchInputConfig {
    pub file_name: String,
}

/// Batch metadata.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub name: String,
    pub display_name: String,
    pub model: String,
    pub state: BatchState,
    pub create_time: String,
    pub end_time: Option<String>,
    pub update_time: String,
}

/// Batch state.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchState {
    BatchStateUnspecified,
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Expired,
}

/// Operation.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub name: String,
    pub done: bool,
    pub error: Option<serde_json::Value>,
    pub response: Option<serde_json::Value>,
}

/// Cached content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct CachedContent {
    pub name: Option<String>,
    pub model: String,
    pub contents: Option<Vec<Content>>,
    pub system_instruction: Option<Content>,
    pub tools: Option<Vec<Tool>>,
    pub ttl: Option<String>,
    pub expire_time: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_request_serialization() {
        let request = InteractionRequest {
            model: Some("models/gemini-1.5-pro".to_string()),
            cached_content: Some("cachedContents/12345".to_string()),
            agent: None,
            input: InteractionInput::Text("Hello".to_string()),
            system_instruction: None,
            previous_interaction_id: None,
            tools: None,
            tool_choice: None,
            generation_config: None,
            safety_settings: None,
            store: None,
            background: None,
            stream: None,
        };

        let json = serde_json::to_value(&request).unwrap();
        
        assert_eq!(json["model"], "models/gemini-1.5-pro");
        assert_eq!(json["cached_content"], "cachedContents/12345");
        assert!(json.get("agent").is_none());
    }

    #[test]
    fn test_function_response_serialization() {
        let resp = FunctionResponse {
            id: Some("call_123".to_string()),
            name: "test_func".to_string(),
            response: serde_json::json!({"foo": "bar"}),
        };
        let part = InteractionPart::FunctionResponse(resp);
        let json = serde_json::to_value(&part).unwrap();
        
        assert_eq!(json["type"], "function_result");
        assert_eq!(json["call_id"], "call_123");
        assert_eq!(json["name"], "test_func");
        assert_eq!(json["result"]["foo"], "bar");
    }
}
