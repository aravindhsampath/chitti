# Chitti: Personal Assistant Daemon (Rust)

## Overview
Chitti is a lean, resilient personal assistant daemon for macOS, written in Rust. It leverages Gemini-3 models via the Google Generative AI API to provide intelligent assistance.

## Core Principles
- **Minimal Abstraction**: Keep the codebase linear and easy to follow.
- **Resilience**: The daemon must handle API failures, network issues, and invalid inputs without crashing.
- **Observability**: Comprehensive logging for debugging and monitoring.
- **Lean Implementation**: Minimal dependencies, focusing on standard Rust patterns.

## Architecture
1. **Daemon Core**: A long-running process that manages the interaction loop and system state.
2. **Communication Layer**:
   - Initial: Terminal-based prompt (REPL).
   - Future: Pluggable channels (e.g., Raycast, WhatsApp, or macOS Menu Bar).
3. **LLM Engine**: Integration with Google's Gemini-3 API.
4. **Configuration**: Environment-based (.env) for API keys and model selection.

## Roadmap
### Phase 1: Foundation (Current)
- [ ] Initialize Rust project with `tracing` for logging.
- [ ] Implement `.env` loading and validation.
- [ ] Create a resilient error-handling framework.
- [ ] Build the core daemon loop.

### Phase 2: Gemini Integration
- [ ] Implement a minimalist Gemini API client using `reqwest`.
- [ ] Support streaming and non-streaming responses.
- [ ] Handle rate limiting and transient network errors.

### Phase 3: Interaction (Terminal)
- [ ] Build a robust CLI interface for the terminal.
- [ ] Support basic commands (e.g., /exit, /clear).

### Phase 4: Expansion (Future)
- [ ] Persistent conversation history.
- [ ] Tool/Plugin system (e.g., calendar, file system).
- [ ] Transition to a true background daemon (launchd).

## Tech Stack
- **Language**: Rust
- **HTTP Client**: `reqwest` (with `tokio` for async)
- **Logging**: `tracing` + `tracing-subscriber`
- **Config**: `dotenvy`
- **Serialization**: `serde`
