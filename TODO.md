# Chitti: Architectural Roadmap & TODOs

Chitti is envisioned as a resilient, omni-channel, omni-provider AI runtime. The current foundation features a decoupled architecture (`Brains` ↔ `Conductor` ↔ `Bridges`). However, to truly support multiple concurrent channels and diverse AI providers without the orchestrator collapsing under edge cases, the core runtime must be solidified.

The following phases outline the roadmap to harden the foundation *before* horizontally expanding to new models and platforms.

## Phase 1: Solidifying the Core Runtime (The Conductor)

- [ ] **Abstract the Tool Registry**
  - **The Problem:** Tool execution (e.g., the `ls` command) is currently hardcoded directly inside the `Conductor`'s run loop.
  - **The Solution:** Create a `CapabilityManager` or `ToolRegistry` trait. The `Conductor` should dynamically load tools at startup. When a model requests a tool, the `Conductor` should route the request to the registry, executing the corresponding logic and returning the result seamlessly. This decouples business logic from orchestration.
  
- [ ] **Multi-Tenant & Multi-Channel Multiplexing**
  - **The Problem:** `main.rs` currently boots exactly *one* bridge based on `config.ui_mode` (`Tui` or `Web`). 
  - **The Solution:** Refactor the initialization to support running multiple bridges concurrently. The `Conductor` should act as a true multiplexer, managing separate `SessionStates` mapped to a `channel_id` + `user_id`. A user should be able to send a message via WebSockets and see the response there, while another process runs the TUI independently.

- [ ] **Persistent & Abstracted Session Memory**
  - **The Problem:** Conversation history is held entirely in RAM (`Vec<InteractionTurn>`). Restarting the server wipes the context.
  - **The Solution:** Introduce a `MemoryStore` trait. Implement a lightweight local storage adapter (e.g., SQLite via `rusqlite` or structured JSON on disk) to persist `interaction_id`s, turn histories, and user settings. This is crucial for multi-session and long-lived server deployments.

- [ ] **Provider Interface Normalization (`BrainEngine`)**
  - **The Problem:** The current `BrainEvent` stream is heavily biased toward Gemini's specific implementation details (e.g., `ThoughtSignature`, `ThoughtDelta`).
  - **The Solution:** Review the event types to ensure they are generic enough to support Anthropic's block-based tool use and OpenAI's streaming structures. Standardize how system prompts, model temperatures, and token usage statistics are passed through the `TurnContext`.

- [ ] **Internal Type Safety & Serialization Overhead**
  - **The Problem:** Relying on `serde_json::Value` and raw Strings for internal routing can lead to silent failures and high overhead.
  - **The Solution:** Define strictly typed Rust structs for all inter-module communication (Bridges -> Conductor -> Brains), reserving raw JSON parsing exclusively for the network boundaries of specific APIs.

## Phase 2: Observability, Security & Ergonomics

- [ ] **Graceful Shutdown & Event Cleanups**
  - Implement `tokio::signal` handlers to gracefully wind down open WebSocket connections, close database handles, and flush logs when the Chitti process receives a `SIGINT` or `SIGTERM`.

- [ ] **Telemetry & Token Tracking**
  - Extract the token usage metadata provided by the Gemini API and bubble it up through the `SystemEvent`s so Bridges can display cost/usage metrics. Implement structured tracing for token latency (Time To First Token).

- [ ] **Secure Secret Management**
  - Move away from relying purely on `.env` for production. Integrate an OS-native secure credential store (e.g., macOS Keychain via the `keyring` crate) to safeguard API keys.

## Phase 3: Horizontal Expansion (Post-Hardening)

*Only proceed with these once Phase 1 & 2 are complete.*

- [ ] **New Brains (Providers)**
  - Implement `AnthropicEngine` using the `BrainEngine` trait.
  - Implement `OpenAIEngine` (or local models via Ollama/Llama.cpp).
  
- [ ] **New Bridges (Channels)**
  - Build a `WebhookBridge` for Discord / Slack / Signal integration.
  - Build a headless Raycast extension bridge.

- [ ] **Advanced Capabilities (Tools)**
  - Implement a `FileEditorTool` for precise codebase modifications.
  - Implement a `BrowserTool` (e.g., headless Puppeteer integration) for deep web scraping beyond generic Google searches.