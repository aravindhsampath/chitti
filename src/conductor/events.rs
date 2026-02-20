use serde_json::Value;
use serde::Serialize;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum UserEvent {
    Input(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SystemEvent {
    Text(String, SessionState),
    ToolCall { name: String, args: Value, state: SessionState },
    Error(String, SessionState),
    RequestApproval { description: String, state: SessionState },
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionState {
    pub model: String,
    pub thinking_level: String,
    pub streaming: bool,
    pub pwd: String,
    pub git_branch: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum BrainEvent {
    TextDelta(String),
    ThoughtDelta(String),
    ToolCall { name: String, id: String, args: Value },
    Complete { interaction_id: Option<String> },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct TurnContext {
    pub prompt: String,
    pub previous_interaction_id: Option<String>,
    pub tool_results: Vec<ToolResult>,
    pub streaming: bool,
    pub thinking_level: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub result: Value,
    pub is_error: bool,
}
