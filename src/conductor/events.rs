use serde_json::Value;

#[derive(Debug, Clone)]
pub enum UserEvent {
    Message(String),
    Command(String), // e.g. "/exit", "/clear"
    Steer(String),   // Steering instruction
    Approve,         // "y"
    Reject,          // "n"
}

#[derive(Debug, Clone)]
pub enum SystemEvent {
    Text(String),
    ToolCall { name: String, args: Value },
    Error(String),
    RequestApproval { description: String },
}

#[derive(Debug, Clone)]
pub enum BrainEvent {
    TextDelta(String),
    ThoughtDelta(String),
    ToolCall { name: String, id: String, args: Value },
    Error(String),
    Complete,
}

#[derive(Debug, Clone)]
pub struct TurnContext {
    pub prompt: String,
    pub previous_interaction_id: Option<String>,
    pub tool_results: Vec<ToolResult>,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub result: Value,
    pub is_error: bool,
}
