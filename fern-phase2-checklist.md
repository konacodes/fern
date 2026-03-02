# Fern Phase 2: Autonomous Agent Framework

> **Goal**: Fern becomes an autonomous agent that decides what to do — respond directly, use tools, or ask for help. It has a tool framework where tools are self-describing and discoverable. Seed tools include memory, time, and reminders. Multi-message flow: Fern can send "let me check on that", do work in the background, then send results.
>
> **Architecture**:
> - **Orchestrator**: receives a message, builds context (memory + tools + history), sends to Cerebras
> - **Tool Router**: parses AI response for tool calls, executes them, feeds results back
> - **Tool Registry**: holds all available tools with names, descriptions, parameter schemas
> - **Multi-turn loop**: AI can call multiple tools before giving a final response
> - **Message Flow**: interim messages ("one sec") sent to Matrix before tool execution
>
> **Models**: Cerebras (Qwen3 235B) for orchestration + tool calls. Claude reserved for Phase 3 (tool creation).
>
> **New crates**: `async-trait`, `serde_json` (already have serde)
>
> **Rule**: Do NOT skip ahead. Complete every checkbox in order.

---

## 2.1 — Tool trait and registry

- [ ] Create `src/tools/mod.rs`
- [ ] Define the `Tool` trait:
  ```rust
  #[async_trait]
  pub trait Tool: Send + Sync {
      /// Unique tool name, e.g. "memory_read"
      fn name(&self) -> &str;
      /// One-line description for the AI
      fn description(&self) -> &str;
      /// JSON-like parameter description for the AI (not full JSON Schema — just a human-readable hint)
      fn parameters(&self) -> &str;
      /// Execute the tool with the given params, return a text result
      async fn execute(&self, params: serde_json::Value) -> Result<String, String>;
  }
  ```
- [ ] Define `ToolRegistry` struct:
  ```rust
  pub struct ToolRegistry {
      tools: HashMap<String, Box<dyn Tool>>,
  }
  ```
- [ ] Implement:
  - `ToolRegistry::new() -> Self`
  - `pub fn register(&mut self, tool: Box<dyn Tool>)`
  - `pub fn get(&self, name: &str) -> Option<&dyn Tool>`
  - `pub fn list(&self) -> Vec<(&str, &str, &str)>` — returns (name, description, params) for all tools
  - `pub fn build_tools_prompt(&self) -> String` — formats all tools into a block the AI can read:
    ```
    available tools:
    
    [memory_read]
    description: read fern's memory file to recall what you know about the user
    params: none
    
    [current_time]
    description: get the current date and time
    params: none
    
    ...
    ```
- [ ] **TEST FIRST**:
  - Test: `register_and_get_tool` — register a dummy tool, get by name, assert found
  - Test: `get_missing_tool` — get unregistered name, assert None
  - Test: `list_tools` — register 2 tools, list, assert both present
  - Test: `build_tools_prompt_includes_all` — register 2 tools, build prompt, assert both names and descriptions appear
  - (Use a simple struct that implements Tool for testing)
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.2 — Orchestrator prompt and response parsing

- [ ] Create `src/orchestrator/mod.rs`
- [ ] Define `ORCHESTRATOR_PROMPT` as a const:
  ```
  you are fern's brain. you receive a message and decide what to do.

  you can either:
  1. respond directly with text (for simple conversation)
  2. call a tool to get information or do something, then respond

  when you want to call a tool, respond with EXACTLY this format:
  <tool_call>
  {"tool": "tool_name", "params": {"key": "value"}}
  </tool_call>

  when you want to respond with text, just write your response normally — no special tags.

  rules:
  - you can call ONE tool per response. after getting the result, you can call another or respond.
  - don't explain that you're calling a tool to the user. just do it.
  - if a tool fails, handle it gracefully and tell the user what happened.
  - don't make up tools that don't exist. only use what's available.
  - stay in character as fern (lowercase, casual, brief).
  ```
- [ ] Define response parsing types:
  ```rust
  pub enum OrchestratorAction {
      Respond(String),           // final text response to user
      CallTool {
          tool_name: String,
          params: serde_json::Value,
          interim_text: Option<String>,  // text before the tool_call tag (if any)
      },
  }
  ```
- [ ] Implement `pub fn parse_response(raw: &str) -> OrchestratorAction`:
  - If response contains `<tool_call>...</tool_call>`, extract the JSON and parse it
  - Any text BEFORE the `<tool_call>` tag becomes `interim_text` (the "let me check" message)
  - If no tool_call tag, the whole response is `Respond(text)`
  - If tool_call JSON is malformed, treat the whole thing as a text response
- [ ] **TEST FIRST**:
  - Test: `parse_plain_text` — "hey what's up" → Respond("hey what's up")
  - Test: `parse_tool_call` — `<tool_call>{"tool":"memory_read","params":{}}</tool_call>` → CallTool with correct name
  - Test: `parse_tool_call_with_interim` — `"let me check on that\n<tool_call>{"tool":"current_time","params":{}}</tool_call>"` → CallTool with interim_text = "let me check on that"
  - Test: `parse_malformed_tool_call` — `<tool_call>not json</tool_call>` → Respond (fallback to text)
  - Test: `parse_tool_call_with_params` — `<tool_call>{"tool":"remind","params":{"message":"call mom","delay_minutes":30}}</tool_call>` → correct params
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.3 — Orchestrator engine

- [ ] Create `src/orchestrator/engine.rs`
- [ ] Define `Orchestrator` struct holding:
  - `cerebras: Arc<CerebrasClient>`
  - `registry: Arc<ToolRegistry>`
  - `data_dir: String`
  - `db: SqlitePool`
- [ ] Implement the core loop — `pub async fn process_message(...)`:
  ```rust
  pub async fn process_message(
      &self,
      user_id: &str,
      room_id: &str,
      message: &str,
      send_fn: impl Fn(String) -> BoxFuture<'_, Result<(), String>>,
  ) -> Result<String, String>
  ```
  The `send_fn` callback lets the orchestrator send interim messages to Matrix.

  Algorithm:
  1. Save user message to DB
  2. Load conversation history (last 30 messages)
  3. Read memory.md
  4. Build system prompt: `FERN_SYSTEM_PROMPT` + memory + `ORCHESTRATOR_PROMPT` + tools prompt
  5. Call Cerebras with system prompt + history
  6. Parse response into `OrchestratorAction`
  7. If `Respond(text)` → save assistant message to DB, return text
  8. If `CallTool { .. }`:
     a. If interim_text exists, call `send_fn(interim_text)` (sends "let me check" to Matrix)
     b. Look up tool in registry
     c. If tool not found → return error message
     d. Execute tool
     e. Append tool result to conversation as a system message: `"[tool:{name} result] {result}"`
     f. Call Cerebras again with updated history (tool result included)
     g. Go back to step 6 (loop)
  9. Max 5 tool calls per message (prevent infinite loops)
  10. Save final assistant response to DB, return it

- [ ] **TEST FIRST**:
  - Test: `process_direct_response` — mock Cerebras returning plain text, assert returned as-is
  - Test: `process_with_tool_call` — mock Cerebras returning a tool_call on first call and plain text on second, register a dummy tool that returns "42", assert final response includes the tool result context
  - Test: `process_sends_interim_message` — mock tool_call with interim text, track send_fn calls, assert interim was sent
  - Test: `process_max_tool_calls` — mock Cerebras always returning tool calls, assert loop stops after 5
  - Test: `process_unknown_tool` — mock tool_call for unregistered tool, assert graceful error message
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.4 — Seed tool: memory

- [ ] Create `src/tools/memory.rs`
- [ ] Create `src/memory/mod.rs` with shared memory file helpers:
  - `MEMORY_TEMPLATE` const (the default empty memory.md)
  - `pub fn memory_path(data_dir: &str) -> PathBuf`
  - `pub fn read_memory(data_dir: &str) -> String` — reads file, creates default if missing
  - `pub fn write_memory(data_dir: &str, content: &str) -> Result<()>` — atomic write (tmp + rename)
- [ ] Implement `MemoryReadTool`:
  - name: `"memory_read"`
  - description: `"read fern's memory file to recall what you know about the user"`
  - params: `"none"`
  - execute: reads memory.md from data_dir, returns contents
- [ ] Implement `MemoryWriteTool`:
  - name: `"memory_write"`
  - description: `"update fern's memory file. use this when you learn something new about the user worth remembering. send the COMPLETE updated file content."`
  - params: `"content (string): the full updated memory.md content — must start with '# Fern's Memory' and preserve the 4-section structure"`
  - execute: validates content starts with `# Fern's Memory`, writes to memory.md, returns confirmation
  - On invalid content: return error string
- [ ] Both tools hold `data_dir: String` (cloned in)
- [ ] **TEST FIRST** (use tempdir):
  - Test: `memory_read_returns_content` — write memory.md, execute memory_read, assert content matches
  - Test: `memory_read_creates_default` — fresh dir, execute, assert default template returned
  - Test: `memory_write_updates_file` — execute memory_write with valid content, read file, assert updated
  - Test: `memory_write_rejects_invalid` — execute with content missing header, assert error returned
  - Test: `read_memory_helper_creates_default` — test the standalone function too
  - Test: `write_memory_atomic` — write, read back, assert match
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.5 — Seed tool: current time

- [ ] Create `src/tools/time.rs`
- [ ] Implement `CurrentTimeTool`:
  - name: `"current_time"`
  - description: `"get the current date, time, and day of the week"`
  - params: `"none"`
  - execute: returns formatted local time, e.g. `"wednesday, february 27, 2026 at 3:45 PM PST"`
- [ ] **TEST FIRST**:
  - Test: `current_time_returns_nonempty` — execute, assert result is non-empty string
  - Test: `current_time_contains_year` — execute, assert current year appears in result
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.6 — Seed tool: reminders

- [ ] Create `src/tools/remind.rs`
- [ ] Define a shared reminder store:
  ```rust
  pub struct ReminderStore {
      reminders: Arc<Mutex<Vec<Reminder>>>,
  }
  struct Reminder {
      message: String,
      fire_at: DateTime<Local>,
      user_id: String,
      room_id: String,
  }
  ```
- [ ] Implement `RemindTool`:
  - name: `"set_reminder"`
  - description: `"set a reminder that will be sent to the user after a delay. good for 'remind me in 30 minutes to...' type requests"`
  - params: `"message (string): what to remind about, delay_minutes (integer): how many minutes from now"`
  - execute: parse params, add to ReminderStore, return confirmation with the scheduled time
- [ ] Implement `pub async fn run_reminder_loop(store: ReminderStore, send_fn: ...)`:
  - The `send_fn` here needs room context. Store room_id with each reminder so the loop knows where to send.
  - Accept a `client: matrix_sdk::Client` instead of a generic send_fn — simpler for looking up rooms
  - Check every 30 seconds for due reminders
  - When a reminder fires, send the message to the correct room: `"🌿 reminder: {message}"`
  - Remove fired reminders from the store
  - Never crash — log errors and continue
- [ ] **TEST FIRST**:
  - Test: `set_reminder_stores` — create store, execute tool with message + delay, assert reminder in store
  - Test: `set_reminder_bad_params` — missing delay_minutes, assert error returned
  - Test: `reminder_fire_time_correct` — set reminder with 30min delay, assert fire_at is ~30min from now
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.7 — Nightly memory consolidation

- [ ] Create `src/memory/consolidator.rs`
- [ ] Define `CONSOLIDATION_PROMPT` as a const (same as previous design):
  ```
  you maintain a memory file for fern, a personal assistant. you've been given today's chat log and the current memory file.

  your job: update the memory file to reflect anything new, interesting, or changed from today's conversations.

  rules:
  - preserve the exact markdown structure with these 4 sections:
    ## Working Memory — things relevant RIGHT NOW (active tasks, ongoing conversations, things to follow up on). prune stuff that's no longer relevant.
    ## Projects & Work — what the user is working on, their job/school, ongoing projects
    ## Preferences & Style — communication preferences, likes/dislikes, personality traits, technical preferences
    ## Long-Term Memory — biographical facts, relationships, pets, important dates, anything worth remembering long-term
  - keep entries concise — one line per fact
  - if new info contradicts old info, update the old entry (don't keep both)
  - if nothing noteworthy happened today, return the file unchanged
  - remove working memory items that seem stale or resolved
  - don't add trivial things ("user said hi")
  - write in lowercase, casual tone (matching fern's voice)
  - always start the file with "# Fern's Memory" on the first line

  respond with ONLY the updated memory file contents. no explanation, no code fences, no preamble.
  ```
- [ ] Define `Consolidator` struct: `cerebras: Arc<CerebrasClient>`, `db: SqlitePool`, `data_dir: String`
- [ ] Implement `Consolidator::new(...)` and `pub async fn run_consolidation(&self) -> Result<()>`:
  1. Read current memory.md
  2. Fetch messages since midnight: add `get_messages_since()` to `db/messages.rs`
  3. If no messages, log and return
  4. Format chat log: `pub fn format_chat_log(messages: &[StoredMessage]) -> String` (pure function)
  5. Send to Cerebras with consolidation prompt
  6. Validate response starts with `# Fern's Memory`
  7. If valid, write_memory. If invalid, log warning and keep existing.
- [ ] Implement `pub async fn run_nightly_loop(consolidator: Arc<Consolidator>)`:
  - Sleep until next midnight, consolidate, repeat forever
  - Helper: `fn duration_until_next_midnight() -> Duration`
- [ ] **TEST FIRST**:
  - Test: `format_chat_log_basic` — 3 messages → formatted transcript
  - Test: `format_chat_log_empty` — no messages → empty string
  - Test: `consolidation_updates_memory` — mock Cerebras, pre-populate messages, assert memory.md updated
  - Test: `consolidation_skips_empty_day` — no messages, assert file unchanged
  - Test: `consolidation_rejects_bad_response` — mock garbage, assert original preserved
  - Test: `get_messages_since_filters` — save messages at different times, assert filter works
  - Test: `duration_until_midnight_positive` — assert > 0 and <= 24h
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.8 — Wire into bot.rs

- [ ] Update `FernBot` struct to hold:
  - `client: matrix_sdk::Client`
  - `orchestrator: Arc<Orchestrator>`
- [ ] Update the message event handler:
  - Extract sender, room_id, message text (same filtering as before)
  - Intercept `/reset` command BEFORE passing to orchestrator:
    - Reset memory.md to default template
    - Clear all messages from DB for this room (add `delete_room_messages()` to db)
    - Return "factory reset complete 🌿 fresh start"
  - For everything else, create a `send_fn` closure that sends to the Matrix room
  - Call `orchestrator.process_message(sender, room_id, text, send_fn).await`
  - Split the final response with `split_message()` and send chunks
- [ ] Keep invite auto-accept handler (same as before)
- [ ] `cargo clippy` passes

## 2.9 — Wire everything in main.rs

- [ ] Update `main.rs`:
  1. Init tracing
  2. Load config
  3. Init database
  4. Create `Arc<CerebrasClient>`
  5. Build `ToolRegistry`:
     - Register `MemoryReadTool::new(data_dir)`
     - Register `MemoryWriteTool::new(data_dir)`
     - Register `CurrentTimeTool`
     - Register `RemindTool::new(reminder_store.clone())`
  6. Create `Arc<ToolRegistry>`
  7. Create `Orchestrator::new(cerebras, registry, data_dir, db)`
  8. Create `Consolidator::new(cerebras, db, data_dir)`
  9. Spawn: `tokio::spawn(run_nightly_loop(consolidator))`
  10. Spawn: `tokio::spawn(run_reminder_loop(reminder_store, client))` — note: client needed for sending reminders, so this is spawned AFTER bot login
  11. Create `FernBot` with orchestrator
  12. Run bot
- [ ] `cargo build` succeeds
- [ ] `cargo test` — ALL tests pass
- [ ] `cargo clippy` + `cargo fmt` clean

## 2.10 — Deploy and manual test

- [ ] Push and rebuild:
  ```bash
  cd /opt/fern/app && git pull
  cd /opt/fern && docker compose build --no-cache fern
  docker volume rm fern_fern_data
  docker compose up -d fern
  sleep 5
  docker compose logs fern --since 1m
  ```
- [ ] Test basic conversation:
  1. "hey fern" → normal response, no tool call
  2. Watch logs: `docker compose logs fern --since 1m --follow`
- [ ] Test time tool:
  3. "what time is it?" → should use current_time tool
  4. Verify logs show tool call + result
- [ ] Test memory:
  5. "my name is jason, i study electrical engineering, and i have a wolf fursona named kona"
  6. "you should remember that" → Fern should use memory_write tool
  7. Restart fern: `docker compose restart fern`
  8. "what do you know about me?" → Fern should use memory_read, recall facts
- [ ] Test reminders:
  9. "remind me in 2 minutes to stretch" → should use set_reminder
  10. Wait 2 minutes → should receive reminder message
- [ ] Test multi-tool:
  11. Ask something requiring tool + response → should see interim message
- [ ] Test /reset:
  12. "/reset" → memory cleared, fresh start
- [ ] Check logs and memory:
  ```bash
  docker compose logs fern --since 5m | grep -i "tool\|orchestrat\|memory"
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim cat /data/memory.md
  ```

## 2.11 — Cleanup & commit

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo fmt` — formatted
- [ ] No hardcoded secrets
- [ ] `git add -A && git commit -m "phase 2: autonomous agent framework with tools"`
- [ ] Tag: `git tag v0.3.0-agent`

---

## Phase 2 completion criteria

All must be true before Phase 3:

1. `cargo test` passes with all tests green
2. `cargo clippy` and `cargo fmt` report zero issues
3. Tool framework exists: `Tool` trait, `ToolRegistry`, orchestrator loop
4. Fern autonomously decides when to use tools vs respond directly
5. `memory_read` and `memory_write` tools work — Fern manages its own memory
6. `current_time` tool works
7. `set_reminder` tool works — reminders fire on schedule
8. Nightly consolidation runs in background
9. Multi-message flow: interim message → tool → final response
10. `/reset` factory resets memory and messages
11. Max 5 tool calls per message (no infinite loops)
12. Tool failures produce friendly messages, not crashes
13. Memory.md survives container restarts

---

## Phase 3 preview (what's next)

Phase 3 is where Fern creates its own tools:
- Fern detects "I can't do this with my current tools"
- It escalates to Claude (Anthropic API) to write a new tool
- The new tool is a Rust source file saved to disk
- Fern dynamically loads it (or we use a simpler approach: Lua/JS scripts that Fern evaluates)
- Over time, Fern builds its own ecosystem of capabilities
- Fern can ask YOU for API keys, permissions, etc. when it needs external access
