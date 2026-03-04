# Chitti Omni-Channel Tiered Memory Architecture

## Core Philosophy
- **Latency First:** Separate conversational state (low-latency, synchronous) from memory management (asynchronous).
- **Tiered Memory Layers:** Identity, Core Memory, Channel Memory, and Topic Memory.
- **Explicit over Implicit:** Explicit control preferred over AI heuristics where possible (e.g., using a `/new` command for topic switching instead of semantic drift detection).

## Memory Tiers

### 1. Identity & Core Facts (File-Based)
- **`SOUL.md`**: Defines Chitti's core personality, rules, and fundamental behavior.
- **`MEMORY.md`**: Long-term memory persisting forever.
    - **User-written**: Explicit instructions and hard preferences from the user.
    - **AI-managed**: Facts and preferences automatically extracted and learned by Chitti over time.
- **Delivery mechanism**: Both files are sent to the AI model via Context Caching APIs. 
- **Update mechanism**: The Async Memory Manager updates `MEMORY.md` by calling an `update_core_memory` tool, which then triggers an invalidation of the context cache.

### 2. Channel Memory (Context Vibe)
- **Structure**: Uses an abstracted tree model (`Channel` -> `Topic/Thread`) to map all integrations to the same mental model.
- Represents the overarching context of a specific platform (e.g., the vibe of `#rust-dev` on Slack vs `WhatsApp`).
- **Native channels (Slack)**: Uses a 150-word concise channel summary, updated asynchronously via a cron job analyzing recent threads.
- **Single-channel comms (WhatsApp/Signal)**: Defaults to `None` or a static descriptor, treating all conversations effectively as threads/topics inside a master channel.

### 3. Topic Memory (Sliding Window with Summary)
- **Hybrid approach**: Maintains a configurable `N` recent raw messages alongside an `AI Summarized Past`.
- **Tidal Summarization**: To prevent summarizing on every turn, the system allows raw messages to grow to `N + K`. Once the threshold is met, the oldest `K` messages are merged into the existing summary.
- **Topic definition**: Established inherently by platform threads (e.g., Slack threads) or explicitly via a manual `/new <topic_name>` command in single-stream channels (e.g., WhatsApp).

---

## System Architecture

### Key Components
1. **The Conductor (Main State Machine)**
    - Focuses on lightning-fast, read-only memory consumption during inference.
    - Appends user and model turns to an `audit_logs` queue immediately after the interaction.
2. **The Memory Manager (Async Worker)**
    - A background Tokio task powered by a fast, cheap model (e.g., `gemini-flash`).
    - Triggered by Conductor events after completion of a turn.
    - Responsibilities: compacting old messages into summaries and extracting core facts for `MEMORY.md`.
3. **Database (SQLite)**
    - Robust local storage for topic states, summaries, and chronological audit logs.
    - Readied for FTS5 so Chitti can eventually use a `search_memory` tool to retrieve older, non-active contexts.

### The Read Path (Solving Race Conditions)
To eliminate race conditions (e.g., when a user sends 3 messages in 5 seconds before the Memory Manager can summarize them), the system uses a **High-Water Mark (Cursor)**.

When generating a response, the Conductor gathers context sequentially without waiting for locks:
1. `SOUL.md` & `MEMORY.md` (Fast via Cache).
2. `Channel Summary` (if applicable).
3. `Topic Summary` (from the `topics` table).
4. `Raw Messages` (fetched from `audit_logs` where `id > last_summarized_log_id`).

*Why this works:* If a user fires rapid messages, the `last_summarized_log_id` hasn't advanced yet. The read path natively pulls all new un-summarized messages directly from the audit log. The context briefly swells but guarantees no lost data.

---

### Database Schema Blueprint

```sql
CREATE TABLE channels (
    id TEXT PRIMARY KEY,
    platform TEXT,
    channel_summary TEXT
);

CREATE TABLE topics (
    id TEXT PRIMARY KEY,
    channel_id TEXT,
    topic_summary TEXT,
    last_summarized_log_id INTEGER -- The High-Water Mark cursor
);

CREATE TABLE audit_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id TEXT,
    topic_id TEXT,
    role TEXT,
    content TEXT,
    artifact_paths TEXT,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

---

### The Turn Lifecycle

**Phase A: The Fast Conversational Loop (Conductor)**
1. **User Input:** Receives message.
2. **Command Check:** If message begins with `/new <topic>`, establish a new entry in `topics` and switch the active session context.
3. **Context Assembly:** Reads from Cache + SQLite (Topic Summary + Unsummarized logs based on cursor).
4. **Inference:** Calls the target LLM and streams the response to the user.
5. **Audit:** Persists the user prompt, model response, tool calls, and artifact paths into `audit_logs`.
6. **Trigger Manager:** Emits an internal event to wake the asynchronous Memory Manager.

**Phase B: The Asynchronous Compaction (Memory Manager)**
1. **Wake Up:** Task receives trigger.
2. **Evaluate Tidal Buffer:** 
    - Queries `audit_logs` for count of messages where `id > last_summarized_log_id`.
    - If `count < (N + K)`, the manager goes back to sleep.
3. **Compaction:**
    - Gathers the oldest `K` messages from the un-summarized pool.
    - Parses text and invokes LLM summarization on attached image artifacts.
    - Prompts the LLM to merge these messages into the existing `topic_summary`.
4. **Core Fact Extraction:**
    - Concurrently asks the LLM: *"Did the user state any permanent facts or preferences in these recent messages?"*
    - If yes, uses a tool to append to `MEMORY.md` and flags the cache for invalidation.
5. **Commit State:**
    - Updates `topic_summary` in SQLite.
    - Advances `last_summarized_log_id` to point to the last compacted message.

---

### Multi-Modal Artifact Handling
- Base64 encoding is explicitly prohibited in SQLite to keep the DB lean.
- Images and heavy attachments are saved locally to `~/.chitti/artifacts/`.
- The `audit_logs` table stores the relative JSON array of file paths.
- During compaction, the Memory Manager converts visual meaning into a textual description, discarding the need to keep the heavy image context in long-term memory.


## Detailed Implementation Plan

This section outlines a step-by-step, incremental path to implementing the Omni-Channel Tiered Memory Architecture. Each step is designed to be small, testable, and independently verifiable, allowing any engineer to pick up a step and execute it without needing full context of the entire system.

### Step 1: Create `SOUL.md` and `MEMORY.md` Foundation
**Goal:** Establish the file-based identity and long-term memory structures.
- **Action:** Create `SOUL.md` and `MEMORY.md` in the root of the project (or a designated config directory like `~/.chitti/`). Add placeholder content (e.g., "You are Chitti..." in SOUL, and "# User Preferences\n\n# AI Managed Facts\n" in MEMORY).
- **Action:** Update the configuration loading (`src/config.rs`) to resolve and store the paths to these files.
- **Action:** Update `src/conductor/mod.rs` (or `TurnContext`) to read these files from disk and inject their contents as system instructions or prepended context into the `ConversationInput` before passing it to `BrainEngine`.
- **Tests to Add:**
  - Unit test in `config.rs` to verify default paths are resolved correctly.
  - Unit test in `conductor/mod.rs` to ensure `SOUL.md` and `MEMORY.md` contents are successfully read and appended to the context sent to a mock `BrainEngine`.

### Step 2: Implement Gemini Context Caching for Identity Files
**Goal:** Optimize token usage by caching `SOUL.md` and `MEMORY.md` using the Gemini Cache API.
- **Action:** Modify `src/brains/gemini/client.rs` to support creating and referencing a `CachedContent` object.
- **Action:** Create a mechanism in the Conductor to check if a valid cache exists for the current file hashes/timestamps. If not, upload the files to the Gemini Cache API and store the returned `cache_name`.
- **Action:** Update the `interaction` builder in `src/brains/gemini/adapter.rs` to accept and use the `cache_name`.
- **Tests to Add:**
  - Mock the Gemini API in `client.rs` to simulate successful cache creation and retrieval.
  - Unit test to ensure the Conductor correctly invalidates/re-uploads the cache when the file modification time of `MEMORY.md` changes.

### Step 3: Setup SQLite and Basic Database Schema
**Goal:** Introduce the `rusqlite` dependency and establish the foundational database tables.
- **Action:** Add `rusqlite` (with `bundled` feature if necessary) and `tokio-rusqlite` to `Cargo.toml`.
- **Action:** Create `src/memory/db.rs` to manage the SQLite connection pool.
- **Action:** Write database initialization code to run `CREATE TABLE IF NOT EXISTS` for `channels`, `topics`, and `audit_logs` (as defined in the schema blueprint). Include FTS5 virtual tables setup for future search capabilities.
- **Tests to Add:**
  - Integration test creating an in-memory SQLite database (`:memory:`), running migrations, and verifying table schemas exist.

### Step 4: Implement Synchronous Audit Logging
**Goal:** Ensure every interaction (user prompt and model response) is saved to the SQLite `audit_logs` table.
- **Action:** Create a function `insert_audit_log` in `src/memory/db.rs`.
- **Action:** Modify the `Conductor`'s run loop. After a successful turn completes (receiving `BrainEvent::Complete`), extract the raw text/tool calls from the turn history and write them to the `audit_logs` table.
- **Action:** Temporarily hardcode `channel_id = "default"` and `topic_id = "default"`.
- **Tests to Add:**
  - Unit test running a mock conversation through the Conductor and verifying that exactly two records (one User, one Model) are inserted into the mock database.

### Step 5: Implement Topic Switching (`/new`) and Routing
**Goal:** Enable explicit topic creation and route subsequent audit logs to the correct `topic_id`.
- **Action:** Update `src/conductor/events.rs` to parse a `/new <topic_name>` command.
- **Action:** Update `Conductor` to intercept `/new`. When detected, insert a new record into the `topics` table and update the active session's `topic_id` in memory.
- **Action:** Ensure `insert_audit_log` uses the session's active `topic_id` instead of "default".
- **Tests to Add:**
  - Test the `/new testing` command updates the Conductor's internal state.
  - Verify that subsequent mock conversation turns are saved to the database with `topic_id = "testing"`.

### Step 6: The Read Path (High-Water Mark Context Assembly)
**Goal:** Fetch raw messages from the database to supply short-term context to the LLM.
- **Action:** Implement `fetch_unsummarized_logs(topic_id)` in `src/memory/db.rs`. This queries `audit_logs` where `id > last_summarized_log_id` for the given topic.
- **Action:** Update the Conductor's `handle_conversation` flow. Before calling the `BrainEngine`, fetch the unsummarized logs from SQLite, fetch the `topic_summary`, and prepend them to the `ConversationInput`.
- **Tests to Add:**
  - Seed an in-memory DB with a mock `topic_summary` and 3 recent `audit_logs`. Run a turn and verify the payload sent to the `BrainEngine` contains both the summary and the 3 recent messages.

### Step 7: Create the Async Memory Manager Skeleton
**Goal:** Set up the background Tokio task that wakes up when the Conductor finishes a turn.
- **Action:** Create `src/memory/manager.rs`. Define a struct `MemoryManager` that runs in a continuous `tokio::spawn` loop, listening to a `mpsc::Receiver`.
- **Action:** In `main.rs`, initialize the `MemoryManager` channel, pass the Sender to the `Conductor`, and spawn the manager task.
- **Action:** Update the Conductor to send a `TriggerMemoryCheck { topic_id }` event down the channel after writing to `audit_logs` (Step 4).
- **Tests to Add:**
  - Unit test validating that the Manager task receives the trigger event when a mock Conductor turn completes.

### Step 8: Implement Tidal Summarization Logic
**Goal:** Make the Memory Manager evaluate the buffer threshold and perform summarization.
- **Action:** In the Memory Manager loop, upon receiving a trigger, query the DB for the count of unsummarized logs for that `topic_id`.
- **Action:** If count >= `N + K` (e.g., 15), fetch the oldest `K` logs and the current `topic_summary`.
- **Action:** Instantiate a background `BrainEngine` (using a cheaper model like `gemini-3-flash`) and send a prompt requesting it to merge the `K` logs into the existing summary.
- **Action:** Upon receiving the new summary, update the `topics` table with the new text and advance the `last_summarized_log_id` to the ID of the `K`th log.
- **Tests to Add:**
  - Integration test: Seed DB with 15 logs. Trigger the manager. Mock the LLM response. Verify that `topic_summary` is updated in the DB and `last_summarized_log_id` is advanced correctly.

### Step 9: Implement Core Fact Extraction (The `update_core_memory` Tool)
**Goal:** Allow the Memory Manager to asynchronously update `MEMORY.md`.
- **Action:** Define a new internal tool `UpdateCoreMemory { action, content }` in `ToolCallPayload`.
- **Action:** During the compaction phase (Step 8), instruct the background LLM to use this tool if it detects new permanent facts.
- **Action:** Implement the tool execution in the Memory Manager to read `MEMORY.md`, append/rewrite the requested content, save the file, and trigger the Conductor to invalidate the Gemini cache (Step 2).
- **Tests to Add:**
  - Mock the background LLM to emit an `UpdateCoreMemory` tool call. Verify that `MEMORY.md` is modified on disk.

### Step 10: Multi-Modal Artifact Handling (Optional/Later Phase)
**Goal:** Safely handle image attachments without bloating SQLite.
- **Action:** Ensure when the Conductor processes a user image upload, it saves the file to `~/.chitti/artifacts/` and stores only the path string in the `audit_logs` table.
- **Action:** Update the Memory Manager's summarization prompt (Step 8) to load the image from disk and include it in the background LLM request so it can summarize the visual content.
- **Tests to Add:**
  - Verify image paths are correctly serialized/deserialized from SQLite.