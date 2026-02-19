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

/// A piece of content in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub parts: Vec<Part>,
}

/// A part of a content object in the Interactions API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text { text: String },
    Image(MediaPart),
    Audio(MediaPart),
    Video(MediaPart),
    Document(MediaPart),
    FunctionCall(FunctionCall),
    FunctionResponse(FunctionResponse),
}

/// A media part (Image, Audio, Video, Document).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MediaPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    pub mime_type: String,
}

/// A predicted function call from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub args: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

/// A result from a function call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub response: serde_json::Value,
}

/// A single turn in a stateless conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Turn {
    pub role: Role,
    pub content: Content,
}

/// Polymorphic input for interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InteractionInput {
    Text(String),
    Parts(Vec<Part>),
    Turns(Vec<Turn>),
}

/// Tool definitions for the model.
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
}

/// Tool choice configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Any,
    Function { name: String },
    None,
}

/// A declaration of a custom function.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Configuration for content generation in the Interactions API.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Thinking levels for Gemini 3 models.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}

/// Request for creating an interaction.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InteractionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub input: InteractionInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
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

/// Response from an interaction.
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
        signature: String 
    },
    #[serde(rename = "thought_signature")]
    ThoughtSignature { 
        #[serde(default)]
        signature: String 
    },
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
    #[serde(other)]
    Unknown,
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
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SafetySetting {
    pub category: SafetyCategory,
    pub threshold: SafetyThreshold,
}

/// Safety categories.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyCategory {
    HateSpeech,
    SexuallyExplicit,
    Harassment,
    DangerousContent,
    CivicIntegrity,
}

/// Safety thresholds.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case")]
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
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileState {
    StateUnspecified,
    Processing,
    Active,
    Failed,
}

/// File source.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
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
#[serde(rename_all = "snake_case")]
pub struct ListFilesResponse {
    pub files: Vec<File>,
    pub next_page_token: Option<String>,
}

/// Batch request.
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub struct BatchRequest {
    pub display_name: String,
    pub input_config: BatchInputConfig,
}

/// Batch input config.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
#[serde(untagged)]
pub enum BatchInputConfig {
    FileName { file_name: String },
}

/// Batch metadata.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub struct Operation {
    pub name: String,
    pub done: bool,
    pub error: Option<serde_json::Value>,
    pub response: Option<serde_json::Value>,
}

/// Cached content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub struct CachedContent {
    pub name: Option<String>,
    pub model: String,
    pub contents: Option<Vec<Content>>,
    pub system_instruction: Option<Content>,
    pub tools: Option<Vec<Tool>>,
    pub ttl: Option<String>,
    pub expire_time: Option<String>,
}
