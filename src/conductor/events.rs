use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum UserEvent {
    Input(String),
    ToggleMemory,
}

#[derive(Debug, Clone, Serialize)]
pub enum SystemEvent {
    Text(String, SessionState),
    Thought(String, SessionState),
    Info(String, SessionState),
    ToolCall {
        name: String,
        args: Value,
        state: SessionState,
    },
    Error(String, SessionState),
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
    ToolCall {
        name: String,
        id: String,
        args: Value,
    },
    Complete {
        interaction_id: Option<String>,
    },
    Error(String),
}

use crate::brains::gemini::types::InteractionInput;

#[derive(Debug, Clone)]
pub struct TurnContext {
    pub input: InteractionInput,
    pub previous_interaction_id: Option<String>,
    pub streaming: bool,
    pub thinking_level: String,
    pub memory_enabled: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub result: Value,
    pub is_error: bool,
}
