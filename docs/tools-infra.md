# Chitti Tool Infrastructure: Bash & CLI Integration

## Objective
Enable Chitti to execute bash commands on the host macOS system to fulfill complex user requests (e.g., "Find all Rust files containing 'Result' and summarize the logic in them").

## Architecture: The "Tool Bridge"

The infrastructure will act as a bridge between Gemini's `FunctionCall` output and the local macOS environment.

### 1. Core Components

#### `ToolExecutor` (Trait)
A trait defining how a tool is executed and its declaration generated.
```rust
pub trait ToolExecutor {
    fn declaration(&self) -> FunctionDeclaration;
    async fn execute(&self, args: &HashMap<String, serde_json::Value>) -> Result<serde_json::Value, ToolError>;
}
```

#### `BashTool` (Implementation)
The primary tool that wraps `std::process::Command`. 
- **Security**: Commands will run in a non-interactive shell. We should implement a "Safelist" or a "User Confirmation" gate for destructive commands (`rm`, `mv`).
- **Context**: Execution happens in the current working directory of the daemon.

#### `ToolRegistry`
A central registry that:
1. Holds a collection of `ToolExecutor`s.
2. Generates the `Vec<Tool>` required by the Gemini Interactions API.
3. Dispatches incoming `FunctionCall`s to the correct executor.

### 2. The Execution Loop (Interactions API)

When Chitti is in a session:
1. **Request**: Chitti sends user input + `tools` (from registry) to Gemini.
2. **Response**: If Gemini returns `FunctionCall`:
    - The `ToolRegistry` identifies the tool (e.g., `execute_bash`).
    - The `ToolExecutor` runs the command.
    - **Result**: The output (stdout/stderr) is wrapped in a `FunctionResponse`.
3. **Follow-up**: Chitti sends the `FunctionResponse` back to Gemini using `previous_interaction_id`.
4. **Final Answer**: Gemini reasons over the CLI output and provides the final text to the user.

## Proposed API Surface

### `execute_bash` Function Declaration
```json
{
  "name": "execute_bash",
  "description": "Execute a bash command on the local macOS system to read files, search code, or manage system state.",
  "parameters": {
    "type": "object",
    "properties": {
      "command": {
        "type": "string",
        "description": "The full bash command to execute (e.g., 'rg \"TODO\" src/')."
      }
    },
    "required": ["command"]
  }
}
```

## Resilience & Safety

### 1. Error Handling
- **Timeout**: Commands must have a hard timeout (e.g., 30s) to prevent the LLM from hanging the daemon with long-running processes.
- **Output Truncation**: If a command returns 1MB of text, Chitti should truncate it before sending to the LLM to preserve token limits.

### 2. User Permission Gate (Phase 1)
For the first iteration, every tool execution should require a manual "Y/n" confirmation in the terminal.
```text
chitti> Find my TODOs.
[Tool Call: execute_bash { command: "rg TODO" }]
Authorize execution? (y/N): 
```

## Implementation Roadmap

### Step 1: Tool Registry Scaffolding
- Define `ToolExecutor` and `ToolRegistry` in `src/gemini/tools/`.
- Implement `execute_bash` with `tokio::process`.

### Step 2: Main Loop Integration
- Update `run_interaction_loop` in `main.rs` to handle recursive tool calls.
- Implement the "Permission Gate" UI.

### Step 3: Environment Context
- Automatically provide environment hints to the model (e.g., `system_instruction` telling the model it's on a Mac and has `rg`, `fd`, and `bat` installed).

## Future Proofing: Parallel Tools
The engine already supports `parallel_calls`. If Gemini asks to run `ls` and `cat README.md` at the same time, the `ToolRegistry` should execute them concurrently using `tokio::join!`.
