# Gemini API Client Architecture (Rust)

## Core Philosophy
We are building a **production-grade, idiomatic Rust client** for the Google Gemini Interactions, Batch, and Caching APIs. This client is the "engine room" for Chitti. It provides **safe, typed, and ergonomic knobs** for every feature the API offers, delegating policy decisions (privacy, persona, tools) to the application layer.

**Guiding Principles:**
1.  **Type Safety over Stringly-Typed**: Use Rust Enums to represent API constraints (e.g., `ThinkingLevel`, `Tool`).
2.  **Builder Pattern**: Interaction requests have 15+ optional parameters. Builders are mandatory for ergonomics.
3.  **Zero-Cost Abstractions**: Map JSON structures directly to Rust structs using `serde` to avoid runtime overhead.
4.  **Async-First**: Deep integration with `tokio` and `futures` for streaming and background tasks.

---

## The 14-Point Feature Checklist

| Feature                                 | Status     | Implementation Strategy                                      |
| :-------------------------------------- | :--------- | :----------------------------------------------------------- |
| **1. Stateful Conversation**            | ✅ Verified | Use `previous_interaction_id` field in `CreateInteractionRequest`. Chitti stores and passes this ID as needed. |
| **2. Stateless Conversation**           | ✅ Verified | Omit `previous_interaction_id` and pass the full conversation history as a list of `Turn` objects in `input`. |
| **3. Search Grounding (Google vs Exa)** | ✅ Verified | **Google**: Pass `tools: [{"type": "google_search"}]`.<br>**Exa**: Pass `tools: [{"type": "function", "name": "search_exa", ...}]`. Client supports dynamic tool selection per request. |
| **4. Tool Calling**                     | ✅ Verified | **Flattened Structure**: `tools: [{"type": "function", "name": "...", "parameters": {...}}]`. Handle `function_call` output and send back `function_result`. |
| **5. Multimodal Content**               | ✅ Verified | Send `input` as an array of parts: `[{"type": "text", ...}, {"type": "image", "data": "base64..."}]`. |
| **6. Structured Output**                | ✅ Verified | Use `response_format` (JSON schema) and `response_mime_type: "application/json"`. |
| **7. Streaming**                        | ✅ Verified | Set `stream: true`. Handle SSE events (`content.delta`, `interaction.start`). |
| **8. Thinking Levels**                  | ✅ Verified | Set `generation_config: { "thinking_level": "low" | "high" }`. |
| **9. URL Context**                      | ✅ Verified | Use `tools: [{"type": "url_context"}]` to let Gemini fetch and read URLs natively. |
| **10. File Attachments**                | ✅ Verified | Use `File API` to upload -> get URI -> send `{"type": "document", "uri": "..."}`. Or send inline base64 for small files. |
| **11. Privacy (No Store)**              | ✅ Verified | Pass `store: bool` in `InteractionRequest`. Default is `false` (privacy-first), but configurable per request. |
| **12. Parallel Calls**                  | ✅ Native   | Gemini models support this natively. Client handles multiple `function_call` blocks in one turn. |
| **13. Caching**                         | ✅ Planned  | **Implicit**: Automatic via shared prefix.<br>**Explicit**: Separate `CachedContents` service to create cache -> get `name` -> pass `cached_content` (field name pending Interactions API beta support, verified in `GenerateContent`). |
| **14. Background/Batch**                | ✅ Verified | Use `POST /v1beta/batches` for non-urgent tasks. Separate `Batch` client module. |

## 1. Module Structure

The library will be organized into functional domains to keep separation of concerns.

```rust
pub mod gemini {
    pub mod client;       // The HTTP client wrapper (Auth, Base URL)
    pub mod interactions; // The heart: Stateful/Stateless chat & streaming
    pub mod batch;        // Background processing
    pub mod caching;      // Explicit context caching
    pub mod files;        // File uploads (prerequisite for multimodal batch/caching)
    pub mod types;        // Shared data structures (Content, Part, Tool)
    pub mod error;        // Structured error handling
}
```

---

## 2. The Interaction Engine (`interactions.rs`)

This is the most complex part. It must handle polymorphism in inputs and streaming outputs.

### Input Polymorphism
The API accepts `string`, `Content[]`, or `Turn[]`. We solve this with an untagged enum to allow flexible APIs for Chitti.

```rust
#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum InteractionInput {
    Text(String),
    Contents(Vec<Content>),
    Turns(Vec<Turn>), // For stateless history replay
}
```

### The Request Builder (The "Knobs")
We use a Typed Builder pattern to ensure valid states (e.g., `model` OR `agent` must be set).

```rust
pub struct InteractionRequestBuilder {
    // Identity
    model: Option<String>,
    agent: Option<String>,
    
    // State
    previous_interaction_id: Option<String>, // Knob: Stateful vs Stateless
    
    // Content
    input: InteractionInput,
    system_instruction: Option<Content>,
    
    // Capabilities (The Bells & Whistles)
    tools: Vec<ToolDefinition>, // Knob: Google Search, Code, or Custom (Exa)
    tool_choice: Option<ToolChoice>,
    
    // Configuration
    generation_config: Option<GenerationConfig>, // Knob: Thinking level, temperature
    safety_settings: Option<Vec<SafetySetting>>,
    
    // Context & Data
    cached_content: Option<String>, // Knob: Explicit Caching Resource Name
    
    // Execution Mode
    stream: bool,      // Knob: Real-time feedback
    store: bool,       // Knob: Privacy (Default false in Chitti, but true in API)
    background: bool,  // Knob: Async interactions
}
```

### Output Streaming (SSE)
The Interactions API uses Server-Sent Events. We need a robust parser to yield `InteractionEvent`s.

```rust
// The Stream yields these events
pub enum InteractionEvent {
    Start(InteractionMetadata),
    StatusUpdate(Status),
    ContentDelta(ContentDelta), // Text chunks, partial tool calls
    ToolCall(FunctionCall),     // Completed tool call requiring client action
    Error(ApiError),
    Complete(InteractionResult),
}

// Logic:
// 1. reqwest::Client::post() -> Response
// 2. Response::bytes_stream()
// 3. Frame decoder (SSE format "data: {}")
// 4. Deserializer -> InteractionEvent
```

---

## 3. Tooling Architecture
We need to support both **Native Tools** (Google runs them) and **Client Tools** (Chitti runs them, like Exa).

```rust
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolDefinition {
    GoogleSearch,   // Native
    CodeExecution,  // Native
    UrlContext,     // Native
    Function(FunctionDeclaration), // Client-side (e.g., Exa)
}

// Chitti's responsibility:
// If Event::ToolCall received -> Match name -> Execute local code -> Send InteractionRequest with FunctionResponse.
```

---

## 4. Batch Processing (`batch.rs`)
For "Do this in the background" tasks.

**Algorithm:**
1.  **Upload**: If input is huge (JSONL), use `files` module to upload first.
2.  **Submit**: `POST /v1beta/batches`.
3.  **Poll/Wait**: The client should offer a `wait_for_completion` helper, but also expose raw status for non-blocking checks.

```rust
pub struct BatchRequest {
    pub model: String,
    pub source: BatchSource, // Inline requests OR File URI
    pub destination: BatchDestination,
}
```

---

## 5. Caching Strategy (`caching.rs`)
Explicit caching requires a lifecycle: Create -> Use -> Expire/Delete.

**Algorithm:**
1.  **Analyze**: Chitti determines if context > 32k tokens (or user explicitly asks).
2.  **Upload**: Media/Docs uploaded via `files` module.
3.  **Create**: `POST /v1beta/cachedContents` with TTL.
4.  **Inject**: The returned `name` is passed to `InteractionRequestBuilder::cached_content()`.

---

## 6. Implementation Plan (Step-by-Step)

### Step 1: The `types` Module
Define all the Serde structs first. This is the foundation.
- `Content`, `Part` (Text, Image, Blob, FunctionCall).
- `ToolDefinition`, `GenerationConfig` (Thinking levels!).

### Step 2: The `Files` Client
We can't do Batch or Caching effectively without uploading files.
- Implement `upload_file` (handling MIME types).

### Step 3: The `Interactions` Client
- Implement the `send` method (Non-streaming).
- Implement the `stream` method (SSE parsing).
- **Crucial**: Ensure `previous_interaction_id` is exposed for the caller to store.

### Step 4: The "Knobs" Integration
- Add the specific fields for `store`, `thinking_level`, `google_search` to the builder.

---

## 7. Future-Proofing for Chitti
This architecture ensures Chitti can simply:
*   "Turn on privacy" -> `builder.store(false)`
*   "Think hard" -> `builder.thinking_level(ThinkingLevel::High)`
*   "Search web" -> `builder.tools(vec![ToolDefinition::GoogleSearch])`
*   "Remember this" -> `builder.cached_content(cache_id)`

This plan decouples the **mechanism** (Rust Client) from the **policy** (Chitti's brain).
