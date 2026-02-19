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
pub struct Content {
    /// The role of the content creator.
    pub role: Option<Role>,
    /// The parts that make up the content.
    pub parts: Vec<Part>,
}

/// A part of a content object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    /// Text part.
    Text(String),
    /// Inline data (base64 encoded).
    InlineData(Blob),
    /// File data (URI reference).
    FileData(FileData),
    /// A predicted function call.
    FunctionCall(FunctionCall),
    /// A result from a function call.
    FunctionResponse(FunctionResponse),
}

/// A blob of data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    /// The IANA standard MIME type of the data.
    pub mime_type: String,
    /// Raw bytes data, base64-encoded.
    pub data: String,
}

/// A reference to data stored in a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileData {
    /// The IANA standard MIME type of the data.
    pub mime_type: String,
    /// The URI of the file.
    pub file_uri: String,
}

/// A predicted function call from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The unique ID of the function call.
    pub id: Option<String>,
    /// The name of the function to call.
    pub name: String,
    /// The arguments to pass to the function.
    pub args: HashMap<String, serde_json::Value>,
    /// Thought signature for maintaining reasoning context.
    pub thought_signature: Option<String>,
}

/// A result from a function call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    /// The unique ID of the function call this response is for.
    pub id: Option<String>,
    /// The name of the function.
    pub name: String,
    /// The result of the function call.
    pub response: serde_json::Value,
}

/// A single turn in a stateless conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// The role of the turn.
    pub role: Role,
    /// The content of the turn.
    pub content: Content,
}

/// Polymorphic input for interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InteractionInput {
    /// Simple text prompt.
    Text(String),
    /// List of content objects.
    Contents(Vec<Content>),
    /// List of turns (for history replay).
    Turns(Vec<Turn>),
}

/// Tool definitions for the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    /// Google Search grounding.
    GoogleSearch,
    /// Python code execution.
    CodeExecution,
    /// Native URL context fetching.
    UrlContext,
    /// Custom function declaration.
    Function {
        #[serde(flatten)]
        declaration: FunctionDeclaration,
    },
}

/// Tool choice configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    /// The model decides which tools to use.
    Auto,
    /// The model must use one of the specified tools.
    Any,
    /// The model must use the specified function.
    Function { name: String },
    /// The model must not use any tools.
    None,
}

/// A declaration of a custom function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    /// The name of the function.
    pub name: String,
    /// A description of the function.
    pub description: String,
    /// The parameters the function accepts, defined as a JSON schema.
    pub parameters: Option<serde_json::Value>,
}

/// Configuration for content generation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Thinking level configuration.
    pub thinking_config: Option<ThinkingConfig>,
    /// Controls the randomness of the output.
    pub temperature: Option<f32>,
    /// The maximum number of tokens to generate.
    pub max_output_tokens: Option<u32>,
    /// The MIME type of the response (e.g., "application/json").
    pub response_mime_type: Option<String>,
    /// The JSON schema for structured output.
    pub response_schema: Option<serde_json::Value>,
    /// Modalitiies to include in the response.
    pub response_modalities: Option<Vec<String>>,
}

/// Thinking configuration for Gemini 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    /// The maximum depth of the model's internal reasoning process.
    pub thinking_level: ThinkingLevel,
}

/// Thinking levels for Gemini 3 models.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    /// Minimal thinking for low latency.
    Minimal,
    /// Low thinking level for faster responses.
    Low,
    /// Balanced thinking level.
    Medium,
    /// Maximum reasoning depth (Default).
    High,
}

/// Request for creating an interaction.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionRequest {
    /// The name of the model to use.
    pub model: Option<String>,
    /// The name of the agent to use.
    pub agent: Option<String>,
    /// The input to the interaction.
    pub input: InteractionInput,
    /// The system instruction.
    pub system_instruction: Option<Content>,
    /// Previous interaction ID for stateful conversation.
    pub previous_interaction_id: Option<String>,
    /// Tools available to the model.
    pub tools: Option<Vec<Tool>>,
    /// Tool choice configuration.
    pub tool_choice: Option<ToolChoice>,
    /// Configuration for generation.
    pub generation_config: Option<GenerationConfig>,
    /// Safety settings.
    pub safety_settings: Option<Vec<SafetySetting>>,
    /// Whether to store the interaction.
    pub store: Option<bool>,
    /// Whether to run the interaction in the background.
    pub background: Option<bool>,
    /// Whether to stream the response.
    pub stream: Option<bool>,
}

/// Response from an interaction.
#[derive(Debug, Clone, Deserialize)]
pub struct InteractionResponse {
    /// The unique ID of the interaction.
    pub id: String,
    /// The status of the interaction.
    pub status: String,
    /// The outputs from the interaction.
    pub outputs: Vec<InteractionOutput>,
    /// Metadata about the interaction.
    pub metadata: Option<serde_json::Value>,
}

/// An output from an interaction.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionOutput {
    /// Text output.
    #[serde(rename_all = "camelCase")]
    Text { text: String },
    /// Predicted function call.
    FunctionCall(FunctionCall),
    /// Result from a search tool.
    SearchTool(serde_json::Value),
    /// Partial text or thoughts (for streaming).
    #[serde(rename_all = "camelCase")]
    ContentDelta { text: String, thought: Option<bool> },
}

/// Events yielded during a streaming interaction.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionEvent {
    /// Interaction started.
    InteractionStart { id: String, metadata: Option<serde_json::Value> },
    /// Status update.
    StatusUpdate { status: String },
    /// Content delta (text chunk).
    ContentDelta { 
        text: String, 
        #[serde(default)]
        thought: bool,
        thought_signature: Option<String>,
    },
    /// Predicted function call.
    FunctionCall(FunctionCall),
    /// Interaction completed.
    InteractionComplete { result: InteractionResponse },
    /// An error occurred.
    Error { error: ApiError },
}

/// Structured API error.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiError {
    pub code: u16,
    pub message: String,
    pub status: String,
}

/// Media resolution levels for Gemini 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaResolution {
    MediaResolutionLow,
    MediaResolutionMedium,
    MediaResolutionHigh,
    MediaResolutionUltraHigh,
}

/// Safety setting for content generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetySetting {
    /// The category of safety to configure.
    pub category: SafetyCategory,
    /// The threshold for the safety filter.
    pub threshold: SafetyThreshold,
}

/// Categories of safety filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyCategory {
    HateSpeech,
    SexuallyExplicit,
    Harassment,
    DangerousContent,
    CivicIntegrity,
}

/// Thresholds for safety filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyThreshold {
    BlockNone,
    BlockOnlyHigh,
    BlockMediumAndAbove,
    BlockLowAndAbove,
}

/// Metadata for a file uploaded to the API.
#[derive(Debug, Clone, Deserialize)]
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
    pub video_metadata: Option<VideoFileMetadata>,
}

/// The state of a file.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileState {
    StateUnspecified,
    Processing,
    Active,
    Failed,
}

/// The source of a file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileSource {
    SourceUnspecified,
    Uploaded,
    Generated,
    Registered,
}

/// Metadata for a video file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoFileMetadata {
    pub video_duration: String,
}

/// Response for listing files.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesResponse {
    pub files: Vec<File>,
    pub next_page_token: Option<String>,
}

/// Request for creating a batch.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    pub display_name: String,
    pub input_config: BatchInputConfig,
}

/// Input configuration for a batch.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum BatchInputConfig {
    FileName { file_name: String },
}

/// Metadata for a batch.
#[derive(Debug, Clone, Deserialize)]
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

/// The state of a batch.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
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

/// A long-running operation.
#[derive(Debug, Clone, Deserialize)]
pub struct Operation {
    pub name: String,
    pub done: bool,
    pub error: Option<serde_json::Value>,
    pub response: Option<serde_json::Value>,
}

/// Metadata for a cached content.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
