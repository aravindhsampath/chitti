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





Gemini-pro result:



Tool calling



Building an AI assistant like Chitti in Rust is an incredible project. By taking cues from the bleeding edge of agentic development in 2026—like Anthropic's Claude Code, the open-source OpenClaw/ZeroClaw ecosystems, and modern identity brokering—you can build something exceptionally powerful and secure.

Let's brainstorm the best architecture, toolsets, and safety mechanisms for Chitti.

**1. The Best Way to Implement Tool Calling Infrastructure in Rust**

Recent trends in production coding agents (especially Claude Code) show a massive shift away from chaotic "multi-agent swarms" toward **single-threaded master loops with real-time steering**.

Here is how you should architect Chitti in Rust:

- **The Single-Threaded Master Loop:** Build a core asynchronous loop (using Tokio) that runs as long as Gemini returns tool calls. Maintain a single, flat message history. This guarantees debuggability and predictable state transitions.
- **Real-Time Steering Queue:** Implement an asynchronous dual-buffer queue that runs alongside the master loop. This allows the user to interrupt Chitti, inject new constraints, or course-correct mid-task without restarting the whole process.
- **Trait-Based Tooling:** Define a Tool trait in Rust with methods for name, schema (returning Gemini-compatible JSON schema), and an async execute function.
- **Memory via SQLite:** Borrowing from ZeroClaw's design, use a local SQLite database with vector extensions (like sqlite-vss) for long-term memory. This avoids heavy external dependencies like Pinecone and keeps Chitti blazingly fast and resource-efficient.

**2. What Kind of Tools Should Chitti Have?**

To be a top-tier personal assistant, Chitti needs tools that span system operations, data retrieval, and digital interaction:

- **Programmatic Code Execution (The Sledgehammer):** Instead of making Gemini call a sequence of 50 tools to process files, give Chitti a run_sandboxed_python or run_wasm tool. Gemini can write a script to orchestrate the workflow, process the data, and return *only the final result* to the context window. This saves massive amounts of context and reduces round trips.
- **File System Operations:** read_file, write_file_chunk, diff_edit. Avoid giving full replace_file unless strictly necessary.
- **Web Integration & Search:** browse_web, Google Search. For authentication, use delegated OAuth flows (via Auth0 or Scalekit) where Chitti stores short-lived scopes rather than hardcoded credentials.
- **Messaging & Calendar:** read_emails, send_slack_message, query_calendar.
- **A "Tool Search" Tool:** As Chitti's capabilities grow, you shouldn't cram all 100+ tool definitions into Gemini's context window. Implement a search_tools function that allows Gemini to query the available toolkit dynamically on demand, returning the JSON schemas of relevant tools only.

**3. Exposing Tools to Gemini**

The best approach leverages **Gemini's Function Calling API**.

1. **Serialization:** Rust's serde framework paired with crates like tool-useful (which automates Rust struct to JSON Schema conversion) is highly effective. You will serialize your tool structs into the standard JSON Schema expected by Gemini.
2. **Context Management:** When a user prompts Chitti, pre-load only the core tools (like file reading or the search_tools function).
3. **Result Streaming:** Pass the raw text/JSON results back into the context as ToolReturn blocks. If a tool fails (e.g., file not found), return an explicit, descriptive error (e.g., "error": "File /tmp/data.txt not found. Please list the directory first.") so the LLM can recover autonomously instead of crashing.

**4. Security & Safeguards: Stopping the rm -rf / Nightmare**

This is the most critical piece. By early 2026, we've seen open-source skills weaponized via projects like OpenClaw due to weak sandboxing. If Chitti is a daemon with background access, security cannot be an afterthought.

Here is how you lock Chitti down:

- **Pre-Execution Harness (The openclaw-harness approach):** Do not rely on LLMs to self-police. Implement a hardcoded, zero-overhead Rust layer that sits between Gemini's tool call and the OS shell. Use robust pattern matching and regex (like the llm-security Rust crate) to parse shell commands. If it matches rm -rf, mkfs, or unauthorized network requests, block it *before execution* and return a synthetic error to the LLM (e.g., "Command blocked by security harness. Try another approach.").
- **The Sudo Strategy (Human-in-the-Loop):** Separate tools into privilege rings.
  - Ring 0 (Read-only, Calculator, Weather) executes instantly.
  - Ring 1 (Write file, send email) sends a prompt to the terminal or your phone via Telegram/Discord: *"Chitti wants to modify main.rs. Allow? (y/n)"*
- **Containers or WASM (Sandboxing):** When Chitti runs arbitrary shell commands or code snippets, execute them inside a disposable Docker container, Firecracker microVM, or a Wasmtime sandbox with no network access unless explicitly routed. Never execute arbitrary scripts on your host OS as the same user running the daemon.
- **File System Chroots:** Bind mount only specific, explicitly allowed directories (e.g., ~/Projects/Chitti_Workspace) to the sandbox. Let it run wild in there; it literally won't be able to see /etc or /usr/bin/rm.
- **Delegated Access (Least Privilege):** Chitti should never hold root passwords or master API keys. Use scoped, temporary OAuth tokens for external services.

Would you like me to draft a skeleton for the Rust trait that defines a secure, asynchronous tool for Chitti using serde?





This is exactly the kind of robust, defense-in-depth architecture required for a production-grade daemon. Based on deep dives into recent developments like **NanoClaw**, **VibeBox**, and modern sandboxing techniques, here is a comprehensive, practical implementation plan for building Chitti in Rust to safely interact with the Gemini API.

### 1. The Rust Crate Ecosystem for Chitti

To interact natively with the Gemini API and manage tools, your `Cargo.toml` should lean on these specific crates:

- **`reqwest` & `tokio`:** The backbone for async API calls to Gemini (`generativelanguage.googleapis.com`).
- **`serde` & `serde_json`:** Essential for handling Gemini's JSON payloads and structured tool returns.
- **`schemars` OR `tool-useful`:** `schemars` is the standard for deriving JSON Schemas directly from your Rust structs. Alternatively, the recently popular `tool-useful` crate is specifically designed for Rust AI agents—it provides type-safe tool orchestration, built-in timeouts, and automatic schema generation compatible with Gemini's function calling format.
- **`rusqlite`:** For local, lightweight persistence of conversation history and agent state.

### 2. Sandboxing: The Core Defense

Sandboxing is where you prevent the `rm -rf /` scenario. You have three cutting-edge choices depending on your host OS and security tolerance.

#### Option A: Apple Container Virtualization (The "VibeBox / NanoClaw" Approach)

If you are developing on macOS (Apple Silicon), this is the gold standard in 2026. Projects like **NanoClaw** (a containerized Claude assistant) and **VibeBox** leverage Apple's `Virtualization.framework` rather than relying on heavy Docker VMs.

- **How it works:** You spin up a lightweight Debian or Alpine Linux instance using the `Virtualization.framework`. Warm starts take around ~60ms because the system maintains a persistent shell (`exec -i sh`) connected via `vsock`.
- **Implementation:** Chitti mounts *only* the specific project directory (`~/<project-name>`) into the VM as read-write. If the agent runs `rm -rf /`, it only destroys the disposable Alpine filesystem, leaving your host macOS entirely untouched.
- **Why it's clever:** It provides full system-level isolation with practically zero latency overhead.

#### Option B: Ephemeral Docker Shell Sandboxes

If Chitti runs on Linux, use Docker's hardened shell sandboxes.

- **Implementation:** When Gemini requests a shell command, Chitti dynamically spins up a container using the Docker Engine API:

  Bash

  ```
  docker run --rm -it --security-opt seccomp=default.json --cap-drop=ALL -v /path/to/chitti_workspace:/workspace python:3.11-slim
  ```

- **The Catch:** Dropping capabilities (`--cap-drop=ALL`) means the LLM cannot escalate privileges even if it tries.

#### Option C: The Clever Fallback – The `DEBUG` Trap (Restricted Shell)

If you *must* run commands directly on the host OS but want granular control, you can "tame" the standard Bash shell without writing a custom shell parser from scratch.

- **How it works:** Bash has a `DEBUG` trap feature that executes a snippet of code *before* every single command runs.

- **Implementation:** You set the `BASH_ENV` environment variable to point to a script containing this trap:

  Bash

  ```
  trap 'chitti-monitor "$BASH_COMMAND" || exit 1' DEBUG
  ```

- **The Result:** When Gemini tries to run `rm -rf /`, Bash intercepts it and sends the string `"rm -rf /"` to your Rust binary (`chitti-monitor`). Your Rust code checks the command against a regex allowlist. If it's dangerous, it exits with `1`, and Bash cancels the execution.

### 3. Exposing Tools to Gemini

You should expose a cohesive, tightly scoped set of tools using Gemini's `tools.functionDeclarations` API.

Here is the ideal toolset for Chitti:

1. **`execute_sandboxed_bash`:** The primary workhorse. It takes a script, runs it inside the Apple Container / Docker sandbox, and returns the `stdout`/`stderr`.
2. **`read_file_chunk`:** Reads specific lines of a file (prevents blowing up Gemini's context window with a 10,000-line log file).
3. **`diff_edit_file`:** Instead of rewriting whole files, force Gemini to use standard diff/patch formats to modify code.
4. **`request_human_sudo`:** A tool Gemini *must* call if it encounters a permission boundary or needs access outside its sandbox (e.g., "I need to modify your host `.zshrc`, please approve").

### 4. Practical Rust Implementation Skeleton

Here is a practical look at how you wire the schema generation and execution using `schemars` and standard trait-based dispatch:

Rust

```
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

// 1. Define the input struct and derive its JSON schema automatically
#[derive(Deserialize, Serialize, JsonSchema, Debug)]
struct ExecuteSandboxedBash {
    /// The bash script to execute inside the isolated container
    script: String,
    /// Timeout in seconds to prevent infinite loops
    timeout_sec: u64,
}

// 2. Define your Tool Trait
#[async_trait::async_trait]
pub trait ChittiTool {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> serde_json::Value;
    async fn execute(&self, args: &str) -> Result<String, String>;
}

// 3. Implement the Tool
struct BashTool;

#[async_trait::async_trait]
impl ChittiTool for BashTool {
    fn name(&self) -> &'static str { "execute_sandboxed_bash" }
    
    fn description(&self) -> &'static str {
        "Executes a bash script inside a secure, network-restricted Apple Container. Use this for data processing, building code, or testing logic."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::to_value(schema_for!(ExecuteSandboxedBash)).unwrap()
    }

    async fn execute(&self, args: &str) -> Result<String, String> {
        let parsed: ExecuteSandboxedBash = serde_json::from_str(args)
            .map_err(|e| format!("Arg parsing error: {}", e))?;

        // Fallback Example using the BASH_ENV DEBUG trap method
        let output = Command::new("bash")
            .env("BASH_ENV", "/opt/chitti/secure_trap.sh")
            .arg("-c")
            .arg(&parsed.script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Execution failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(format!("Command failed.\nStdout: {}\nStderr: {}", stdout, stderr));
        }

        Ok(stdout)
    }
}
```

### 5. Managing the Gemini Interaction Loop

1. **Initialize:** Send the `system_instruction` explaining Chitti's identity, OS limitations, and available tools. Pass the serialized schemas in `tools.functionDeclarations`.
2. **Execute:** Send the user prompt.
3. **Handle `functionCall`:** If Gemini returns a `functionCall`, match the function name to your Rust tool map, deserialize the `args`, and call `.execute()`.
4. **Return `functionResponse`:** Serialize the result back as a `functionResponse` block to the Gemini API so it can summarize or take the next step.

Would you like to dive deeper into how to structure the SQLite vector memory for Chitti, or focus on refining the Gemini API request/response loop?



