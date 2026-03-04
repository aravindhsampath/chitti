use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserEvent {
    Input(String),
    ToggleMemory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    User,
    Model,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args")]
pub enum ToolCallPayload {
    Ls { path: Option<String> },
    UpdateCoreMemory { action: String, content: String },
    Unknown { name: String, raw_args: String },
}

impl std::fmt::Display for ToolCallPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCallPayload::Ls { path } => write!(f, "ls path={:?}", path),
            ToolCallPayload::UpdateCoreMemory { action, content } => write!(
                f,
                "update_core_memory action={} content={}",
                action, content
            ),
            ToolCallPayload::Unknown { name, raw_args } => write!(f, "{} args={}", name, raw_args),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", content = "result")]
pub enum ToolResponsePayload {
    Ls {
        entries: Result<Vec<String>, String>,
    },
    Unknown {
        result: String,
    },
    UpdateCoreMemory {
        result: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub payload: ToolCallPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub id: String,
    pub payload: ToolResponsePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePart {
    Text { text: String },
    Thought { signature: String, summary: String },
    ToolCall(ToolCall),
    ToolResponse(ToolResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: MessageRole,
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConversationInput {
    Text(String),
    Parts(Vec<MessagePart>),
    Turns(Vec<ConversationTurn>),
}

#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent {
    Text(String, SessionState),
    Thought(String, SessionState),
    Info(String, SessionState),
    ToolCall {
        payload: ToolCallPayload,
        state: SessionState,
    },
    Error(String, SessionState),
    Debug(String, SessionState),
    RequestApproval {
        description: String,
        state: SessionState,
    },
    Ready(SessionState),
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionState {
    pub model: String,
    pub thinking_level: String,
    pub dev_mode: bool,
    pub streaming: bool,
    pub memory_enabled: bool,
    pub pwd: String,
    pub git_branch: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum BrainEvent {
    TextDelta(String),
    ThoughtDelta(String),
    ThoughtSignature(String),
    ToolCall(ToolCall),
    Complete { interaction_id: Option<String> },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct TurnContext {
    pub input: ConversationInput,
    pub previous_interaction_id: Option<String>,
    pub streaming: bool,
    pub thinking_level: String,
    pub memory_enabled: bool,
    pub system_instruction: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolResult {
    pub call_id: String,
    pub payload: ToolResponsePayload,
    pub is_error: bool,
}
