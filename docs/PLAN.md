# Chitti: Personal Assistant Daemon (Rust)

## Overview
Chitti is a lean, resilient personal assistant daemon for macOS, written in Rust. It leverages Gemini-3 models via the Google Generative AI API to provide intelligent assistance.

## Core Principles
- **Minimal Abstraction**: Keep the codebase linear and easy to follow.
- **Resilience**: The daemon must handle API failures, network issues, and invalid inputs without crashing.
- **Observability**: Comprehensive logging for debugging and monitoring using `tracing`.
- **Lean Implementation**: Minimal dependencies, focusing on standard Rust patterns.
- **Decoupled Architecture**: Omni-channel and Omni-AI-provider ready.

## Architecture
1.  **Conductor (State Machine)**: The central orchestrator that manages the interaction lifecycle, tool execution, and user steering.
2.  **CommBridge (Frontend)**: Abstract interface for communication channels.
    -   `TuiBridge`: Current terminal implementation.
    -   `WhatsAppBridge`, `SlackBridge`: Planned future channels.
3.  **BrainEngine (AI Engine)**: Abstract interface for AI providers.
    -   `GeminiEngine`: Proven implementation using the Gemini Interactions API.
4.  **ToolRegistry**: Manages local capabilities (e.g., `BashTool`).

## Roadmap

### Phase 1: Foundation (Completed)
- [x] Initialize Rust project with `tracing` for logging.
- [x] Implement `.env` loading and validation.
- [x] Create a resilient error-handling framework (`GeminiError`).
- [x] Implement Gemini Interactions API with streaming and state.

### Phase 2: Omni-Channel Refactor (Completed)
- [x] Decouple Main loop into the `Conductor`.
- [x] Define `CommBridge` and `BrainEngine` traits.
- [x] Migrate TUI logic to `TuiBridge`.
- [x] Implement `BashTool` for local command execution.
- [x] Add **Manual Approval Gate** for tool execution.
- [x] Implement **Steering Support** during turn boundaries.

### Phase 3: Polish & Expansion (Current)
- [x] Demonstration of channel-agnosticism via `MockBridge`.
- [ ] Implement persistent conversation history.
- [ ] Add more local tools (e.g., File Editor, Browser Automation).
- [ ] Implement Raycast or WhatsApp bridge.

### Phase 4: Production Daemon
- [ ] Transition to a true background daemon (`launchd`).
- [ ] Implement secure credential storage (macOS Keychain).

## Tech Stack
- **Language**: Rust
- **HTTP Client**: `reqwest` (with `tokio` for async)
- **Logging**: `tracing` + `tracing-subscriber`
- **Config**: `dotenvy`
- **Serialization**: `serde` + `serde_json`
- **Async Utility**: `futures-util`, `async-trait`, `tokio-util`
