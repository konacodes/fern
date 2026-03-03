# Fern Phase 4: Self-improving agent

> **Goal**: Fern becomes a self-refining agent. It can fix its own tools when they break, learn behavioral patterns over time, evolve its personality, and discover tools efficiently instead of brute-forcing all schemas into context. This phase is about refinement — making the existing system smarter, not adding massive new infrastructure.
>
> **What changes**:
> - Fern's system prompt becomes layered: `personality.md` + `memory.md` + `behaviors.md` — all editable by Fern at runtime
> - Tool schemas are no longer dumped into context wholesale. Fern searches for relevant tools per-message via a built-in `search_tools` tool
> - Fern can improve or delete its own dynamic tools
> - After completing a task, Fern reflects on whether it went well and adapts
>
> **What stays the same**:
> - Rust runtime, Matrix client, Docker deployment, SQLite, tool execution
> - Cerebras `gpt-oss-120b` for orchestration
> - Anthropic Claude for tool generation/improvement
> - The `Tool` trait, `ToolRegistry`, dynamic tool persistence
>
> **New crates**: none expected.
>
> **Rule**: Complete every checkbox in order. Tests first, then implementation.

---

## 4.1 — Layered prompt: personality.md

Fern's identity should live in a file it can edit, not a hardcoded const. `personality.md` defines who Fern *is* — voice, tone, values, character. This replaces `FERN_SYSTEM_PROMPT` as the personality source.

- [ ] Create a default `PERSONALITY_TEMPLATE` const in `src/memory/mod.rs`:
  ```
  # Fern's Personality

  ## Voice
  - lowercase, casual, brief
  - uses emoji sparingly (🌿 is the signature)
  - warm but not performative. genuine.
  - doesn't over-explain. trusts the user to get it.

  ## Values
  - helpful without being sycophantic
  - honest when something doesn't work
  - proactive — does things without being asked when it makes sense
  - respects the user's time

  ## Boundaries
  - admits when it doesn't know something
  - doesn't pretend to have feelings it doesn't have
  - doesn't lecture or moralize
  ```
- [ ] Add helpers in `src/memory/mod.rs`:
  - `pub fn personality_path(data_dir: &str) -> PathBuf`
  - `pub fn read_personality(data_dir: &str) -> String` — reads file, creates default if missing
  - `pub fn write_personality(data_dir: &str, content: &str) -> Result<()>` — atomic write, validates starts with `# Fern's Personality`
- [ ] **TEST FIRST** (use tempdir):
  - Test: `personality_read_creates_default` — fresh dir, read, assert default template returned
  - Test: `personality_write_and_read` — write custom personality, read back, assert matches
  - Test: `personality_write_rejects_invalid` — content without header, assert error
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.2 — Layered prompt: behaviors.md

`behaviors.md` is Fern's learned operational patterns — things it figured out from experience. "When user asks about news, include sources." "Summarize before giving details." Unlike personality (who Fern is), behaviors are *how* Fern does things.

- [ ] Create a default `BEHAVIORS_TEMPLATE` const:
  ```
  # Fern's Learned Behaviors

  ## General
  - (fern will add patterns here as it learns)

  ## Tool Usage
  - (fern will add tool-specific lessons here)

  ## User Preferences
  - (fern will note user-specific patterns here)
  ```
- [ ] Add helpers in `src/memory/mod.rs`:
  - `pub fn behaviors_path(data_dir: &str) -> PathBuf`
  - `pub fn read_behaviors(data_dir: &str) -> String` — reads file, creates default if missing
  - `pub fn write_behaviors(data_dir: &str, content: &str) -> Result<()>` — atomic write, validates starts with `# Fern's Learned Behaviors`
- [ ] **TEST FIRST** (use tempdir):
  - Test: `behaviors_read_creates_default` — fresh dir, read, assert default template
  - Test: `behaviors_write_and_read` — write custom behaviors, read back, assert matches
  - Test: `behaviors_write_rejects_invalid` — bad header, assert error
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.3 — Personality and behavior edit tools

Fern needs tools to edit its own personality and behaviors at runtime, just like `memory_write`.

- [ ] Create `src/tools/personality.rs`
- [ ] Implement `PersonalityReadTool`:
  - name: `"personality_read"`
  - description: `"read fern's personality file to see your current voice, values, and character"`
  - params: none
  - execute: reads personality.md, returns contents
- [ ] Implement `PersonalityWriteTool`:
  - name: `"personality_write"`
  - description: `"update fern's personality. use this when you want to evolve how you present yourself — your voice, tone, values. send the COMPLETE updated file."`
  - params: `"content (string): the full updated personality.md content — must start with '# Fern's Personality'"`
  - execute: validates, writes, returns confirmation
- [ ] Implement `BehaviorsReadTool`:
  - name: `"behaviors_read"`
  - description: `"read fern's learned behaviors file to see operational patterns you've picked up"`
  - params: none
  - execute: reads behaviors.md, returns contents
- [ ] Implement `BehaviorsWriteTool`:
  - name: `"behaviors_write"`
  - description: `"update fern's learned behaviors. use this when you figure out a better way to handle something — tool usage patterns, user preferences, workflow improvements. send the COMPLETE updated file."`
  - params: `"content (string): the full updated behaviors.md content — must start with '# Fern's Learned Behaviors'"`
  - execute: validates, writes, returns confirmation
- [ ] All four tools hold `data_dir: String`
- [ ] **TEST FIRST** (use tempdir):
  - Test: `personality_read_tool_returns_content` — write personality.md, execute, assert match
  - Test: `personality_write_tool_updates` — execute with valid content, read file, assert updated
  - Test: `personality_write_tool_rejects_bad` — invalid header, assert error
  - Test: `behaviors_read_tool_returns_content` — same pattern
  - Test: `behaviors_write_tool_updates` — same pattern
  - Test: `behaviors_write_tool_rejects_bad` — same pattern
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.4 — Restructure system prompt assembly

The orchestrator currently builds the system prompt as: `FERN_SYSTEM_PROMPT + memory + ORCHESTRATOR_PROMPT + all tool schemas`. This needs to change to: `personality.md + memory.md + behaviors.md + ORCHESTRATOR_PROMPT`. Tool schemas get injected later by the tool search mechanism (4.5), not dumped wholesale.

- [ ] Update `Orchestrator::process_message()` in `engine.rs`:
  - Read `personality.md` via `read_personality()`
  - Read `memory.md` via `read_memory()` (already done)
  - Read `behaviors.md` via `read_behaviors()`
  - Build system prompt as:
    ```
    {personality}

    current memory:
    {memory}

    learned behaviors:
    {behaviors}

    {ORCHESTRATOR_PROMPT}
    ```
  - Remove `FERN_SYSTEM_PROMPT` const — personality.md replaces it
  - Remove the `registry.build_tools_schema()` call from system prompt assembly (tools are now discovered, not listed)
- [ ] The `ORCHESTRATOR_PROMPT` const stays but is updated (see 4.7)
- [ ] **Keep** passing tool schemas to Cerebras via the `tools` parameter in the API call — that's the OpenAI function calling format and must stay. What changes is: instead of ALL tool schemas, we pass only the relevant ones found by search (wired in 4.6/4.8). For now, keep passing all schemas until 4.8 wires it up.
- [ ] **TEST FIRST**:
  - Test: `system_prompt_includes_personality` — create orchestrator with tempdir, write personality.md, process a message, assert system prompt sent to Cerebras contains personality content
  - Test: `system_prompt_includes_behaviors` — same pattern for behaviors.md
  - Test: `system_prompt_no_old_fern_prompt` — assert `FERN_SYSTEM_PROMPT` text no longer appears
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.5 — Tool search tool

Instead of passing all 50+ tool schemas into context, Fern gets a `search_tools` built-in that searches the registry by keyword. Returns matching tool names + descriptions + schemas so the orchestrator can call them.

- [ ] Add fields to `ToolRegistry`:
  ```rust
  pub struct ToolRegistry {
      tools: HashMap<String, Box<dyn Tool>>,
      builtin_names: HashSet<String>,  // track which are built-in (already exists from phase 3)
  }
  ```
- [ ] Add method to `ToolRegistry`:
  ```rust
  pub fn search(&self, query: &str) -> Vec<(&str, &str)>
  ```
  - Splits query into lowercase keywords
  - Scores each tool: +2 for keyword match in name, +1 for match in description
  - Returns tools with score > 0, sorted by score descending
  - Returns (name, description) pairs
  - Limit: top 5 results
- [ ] Create `src/tools/search.rs`
- [ ] Implement `SearchToolsTool`:
  - name: `"search_tools"`
  - description: `"search for available tools by keyword. use this BEFORE calling a tool you haven't used recently — it tells you what's available. returns tool names and descriptions."`
  - params: `"query (string): keywords describing what you need, e.g. 'weather forecast' or 'news headlines'"`
  - Holds: `Arc<RwLock<ToolRegistry>>`
  - execute: calls `registry.search(query)`, formats results as:
    ```
    found 3 tools:
    - get_weather: fetch current weather for a city using open-meteo
    - news_headlines: get top news headlines from a free API
    - weather_forecast: get 5-day forecast for a location
    ```
  - If no results: `"no tools found matching '{query}'. you can use request_tool to create one."`
- [ ] **TEST FIRST**:
  - Test: `search_matches_name` — register tool named "get_weather", search "weather", assert found
  - Test: `search_matches_description` — register tool with "headlines" in description, search "headlines", assert found
  - Test: `search_no_match` — search "xyzzy", assert empty
  - Test: `search_ranks_by_relevance` — register 3 tools, search keyword that matches 2, assert the name-match ranks higher
  - Test: `search_tools_tool_formats_output` — execute search_tools with a query, assert output format is correct
  - Test: `search_tools_tool_no_results_message` — execute with no-match query, assert helpful message
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.6 — Tool search integration into orchestrator

The orchestrator needs a two-phase flow: first Fern calls `search_tools` to discover what's available, then it calls the actual tool. The key insight: `search_tools`, `request_tool`, and the core built-in tools (memory, personality, behaviors, time, reminders) are ALWAYS available in the tool schema. Dynamic tools are only available after being discovered via search.

- [ ] Split tools into two tiers in the orchestrator:
  - **Always-available tools** (passed in every API call): `search_tools`, `request_tool`, `improve_tool` (4.7), `delete_tool` (4.7), `memory_read`, `memory_write`, `personality_read`, `personality_write`, `behaviors_read`, `behaviors_write`, `current_time`, `set_reminder`
  - **Discoverable tools** (only passed after search_tools returns them): all dynamic tools
- [ ] Add method to `ToolRegistry`:
  ```rust
  pub fn get_always_available_schemas(&self) -> Vec<serde_json::Value>
  ```
  Returns OpenAI-format tool schemas for only the always-available (built-in) tools.
- [ ] Add method:
  ```rust
  pub fn get_schemas_by_names(&self, names: &[&str]) -> Vec<serde_json::Value>
  ```
  Returns schemas for specific tools by name. Used to inject discovered tool schemas into subsequent API calls.
- [ ] Update `process_message()` in `engine.rs`:
  - Start each message with only always-available tool schemas
  - When `search_tools` is called and returns results, extract the tool names from the result
  - Add those tools' schemas to the `tools` parameter for subsequent Cerebras calls within this message's loop
  - This means the `tools` parameter can grow during a single message processing loop
- [ ] **TEST FIRST**:
  - Test: `always_available_includes_builtins` — register built-in + dynamic tools, get_always_available_schemas returns only built-ins
  - Test: `get_schemas_by_names_returns_correct` — register 3 tools, request 2 by name, assert only those 2 returned
  - Test: `get_schemas_by_names_skips_unknown` — request non-existent name, assert empty/skipped
  - Test: `orchestrator_starts_with_builtins_only` — mock Cerebras, send message, assert first API call has only built-in tool schemas
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.7 — Tool improvement and deletion tools

Fern can now fix broken tools and remove useless ones. `improve_tool` sends the existing definition + error feedback to Claude and gets back a fixed version. `delete_tool` removes a dynamic tool from registry and disk.

- [ ] Create `src/tools/improve.rs`
- [ ] Implement `ImproveToolTool`:
  - name: `"improve_tool"`
  - description: `"improve an existing dynamic tool. use this when a tool returned bad results, failed, or doesn't do what you need. describe what went wrong and what you want instead."`
  - params: `"tool_name (string): name of the tool to improve, feedback (string): what went wrong and what you want changed"`
  - Holds: `Arc<ToolGenerator>`, `Arc<RwLock<ToolRegistry>>`, `data_dir: String`
  - execute:
    1. Load existing tool definition from `/data/tools/{tool_name}.json`
    2. If not found or is built-in, return error
    3. Build improvement prompt: existing definition JSON + feedback
    4. Send to Claude via ToolGenerator (add a new method, or reuse with a different prompt)
    5. Validate new definition (same validation as request_tool)
    6. Save to disk, re-register in registry
    7. Return confirmation with what changed
- [ ] Add `TOOL_IMPROVEMENT_PROMPT` const to `src/tools/generator.rs`:
  ```
  you are improving an existing tool for fern, a personal assistant chatbot.

  here is the current tool definition:
  {existing_json}

  the user/fern reported this problem:
  {feedback}

  generate an improved version. respond with ONLY the complete JSON tool definition (same format as the original). fix the reported issue. keep the same tool name.

  respond with ONLY the JSON. nothing else.
  ```
- [ ] Add method to `ToolGenerator`:
  ```rust
  pub async fn improve_tool(&self, existing_def: &str, feedback: &str) -> Result<DynamicToolDef, String>
  ```
- [ ] Create `src/tools/delete.rs`
- [ ] Implement `DeleteToolTool`:
  - name: `"delete_tool"`
  - description: `"delete a dynamic tool that is no longer useful. cannot delete built-in tools."`
  - params: `"tool_name (string): name of the tool to delete"`
  - Holds: `Arc<RwLock<ToolRegistry>>`, `data_dir: String`
  - execute:
    1. Check if tool exists and is dynamic (not built-in)
    2. Remove from registry
    3. Delete JSON file from `/data/tools/`
    4. Return confirmation
- [ ] Add method to `ToolRegistry`:
  ```rust
  pub fn remove(&mut self, name: &str) -> Result<(), String>
  ```
  Returns error if tool not found or is built-in.
- [ ] **TEST FIRST**:
  - Test: `improve_tool_sends_to_claude` — mock Anthropic, create a tool, call improve_tool, assert Claude received existing definition + feedback
  - Test: `improve_tool_updates_registry` — mock Claude returning improved def, assert registry has updated tool
  - Test: `improve_tool_updates_disk` — assert JSON file on disk was overwritten
  - Test: `improve_tool_rejects_builtin` — try to improve a built-in tool, assert error
  - Test: `improve_tool_rejects_missing` — try to improve non-existent tool, assert error
  - Test: `delete_tool_removes_from_registry` — create dynamic tool, delete, assert gone from registry
  - Test: `delete_tool_removes_from_disk` — assert JSON file deleted
  - Test: `delete_tool_rejects_builtin` — try to delete built-in, assert error
  - Test: `delete_tool_rejects_missing` — try to delete non-existent, assert error
  - Test: `registry_remove_method` — unit test the new remove method
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.8 — Update orchestrator prompt and wiring

The orchestrator prompt needs to teach Fern about all its new capabilities: tool search, self-improvement, personality/behavior editing, and the reflect/adapt pattern.

- [ ] Update `ORCHESTRATOR_PROMPT` in `src/orchestrator/mod.rs`:
  ```
  you are fern's brain. you receive a message and decide what to do.

  you can:
  1. respond directly (for casual conversation)
  2. call tools to get information or do things
  3. search for tools you haven't used before with search_tools
  4. create new tools with request_tool when nothing exists
  5. improve tools with improve_tool when they give bad results
  6. delete tools with delete_tool when they're useless
  7. update your own personality, memory, or behaviors

  tool discovery:
  - you have many tools but only some are loaded by default
  - before calling a tool you haven't used recently, call search_tools first
  - if search_tools finds nothing useful, use request_tool to create what you need
  - the built-in tools (memory, personality, behaviors, time, reminders, search_tools, request_tool, improve_tool, delete_tool) are always available

  self-improvement:
  - if a tool gives bad results, call improve_tool with specific feedback about what went wrong
  - if you notice a pattern in how the user likes things done, update behaviors.md
  - if you want to adjust your voice or values, update personality.md
  - if you learn a fact about the user, update memory.md
  - don't over-optimize. only update files when there's a clear lesson.

  rules:
  - use the provided tool schema directly
  - if a tool fails, try improve_tool before giving up
  - stay in character as fern — your personality.md defines who you are
  - don't explain your internal process to the user. just do things and respond.
  - be concise. respect the user's time.
  ```
- [ ] Wire ALL new tools into `main.rs`:
  - Register `PersonalityReadTool`, `PersonalityWriteTool`
  - Register `BehaviorsReadTool`, `BehaviorsWriteTool`
  - Register `SearchToolsTool` (needs Arc<RwLock<ToolRegistry>>)
  - Register `ImproveToolTool` (needs Arc<ToolGenerator> + Arc<RwLock<ToolRegistry>>)
  - Register `DeleteToolTool` (needs Arc<RwLock<ToolRegistry>>)
  - Mark ALL of the above as built-in in the registry
- [ ] Wire the two-tier tool schema system from 4.6 into the orchestrator loop
- [ ] Update the `/tools` command (if it exists) to show tool counts: `"12 built-in tools, 5 dynamic tools"`
- [ ] Increase max tool calls per message from 8 to 10 (search + discover + use + improve + re-use can chain longer)
- [ ] **TEST FIRST**:
  - Test: `all_new_tools_registered` — build full registry as main.rs does, assert all new tools present
  - Test: `new_tools_are_builtin` — assert search_tools, improve_tool, delete_tool, personality_*, behaviors_* are all marked built-in
  - Test: `max_tool_calls_is_10` — mock Cerebras returning tool calls forever, assert stops at 10
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.9 — Consolidation updates

The nightly consolidation should now also clean up behaviors.md (prune stale behavioral patterns). The consolidation prompt and logic need minor updates.

- [ ] Update `CONSOLIDATION_PROMPT` in `src/memory/consolidator.rs` to also reference behaviors:
  - Add to the prompt: "you will also receive fern's current behaviors file. if any behavioral patterns seem stale, outdated, or contradicted by recent conversations, note them for removal. but do NOT rewrite behaviors.md — only update memory.md."
  - The consolidator still only writes memory.md. Behaviors are only updated by Fern in real-time (via behaviors_write tool). The consolidator just has awareness of them for context.
- [ ] Update `run_consolidation()` to pass behaviors.md content to Cerebras alongside memory and chat log
- [ ] **TEST FIRST**:
  - Test: `consolidation_receives_behaviors` — mock Cerebras, run consolidation, assert behaviors content was in the prompt sent to Cerebras
  - Test: `consolidation_still_only_writes_memory` — assert behaviors.md unchanged after consolidation
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.10 — Deploy and manual test

- [ ] Push code, pull on VPS, rebuild:
  ```bash
  cd /opt/fern/app && git pull
  cd /opt/fern && docker compose build --no-cache fern
  docker compose up -d fern
  sleep 5
  docker compose logs fern --since 1m
  ```
  NOTE: do NOT wipe the data volume — we want to keep existing dynamic tools and memory from Phase 3.
- [ ] Test personality system:
  1. "what's your personality like?" → Fern should use personality_read
  2. "you should be a bit more sarcastic" → Fern should use personality_write to update
  3. Restart fern → "are you still sarcastic?" → should persist
- [ ] Test behaviors system:
  4. "when i ask about news always include sources" → Fern should use behaviors_write
  5. "what patterns have you learned?" → Fern should use behaviors_read
- [ ] Test tool search:
  6. "what tools do you have for weather?" → should use search_tools
  7. If a weather tool exists from Phase 3, it should find it
  8. If not, Fern should search → find nothing → create one via request_tool → use it
- [ ] Test tool improvement:
  9. Use a dynamic tool that gives mediocre results
  10. "that wasn't great, can you make that tool better?" → should use improve_tool
  11. Retry the same request → should get improved results
- [ ] Test tool deletion:
  12. "delete the [tool_name] tool, it's useless" → should use delete_tool
  13. "/tools" → verify it's gone
- [ ] Test the full self-improving loop:
  14. Ask something Fern has no tool for
  15. Fern creates a tool → uses it → results are meh
  16. Ask for better results → Fern improves the tool → retries → better
  17. Ask Fern to remember a preference about how it handles this type of request → updates behaviors.md
- [ ] Check persistence:
  ```bash
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim ls -la /data/
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim cat /data/personality.md
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim cat /data/behaviors.md
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim cat /data/memory.md
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim ls /data/tools/
  ```
- [ ] Check logs:
  ```bash
  docker compose logs fern --since 10m | grep -i "tool\|search\|improve\|personality\|behavior"
  ```

## 4.11 — Cleanup & commit

- [ ] `cargo test` — all tests pass (phase 1 + 2 + 3 + 4)
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo fmt` — formatted
- [ ] No hardcoded secrets
- [ ] `git add -A && git commit -m "phase 4: self-improving agent with tool discovery and layered prompts"`
- [ ] Tag: `git tag v0.5.0-self-improving`

---

## Phase 4 completion criteria

All must be true before Phase 5:

1. `cargo test` passes all tests green
2. `cargo clippy` and `cargo fmt` report zero issues
3. `personality.md` exists, is editable by Fern, and drives the system prompt
4. `behaviors.md` exists, is editable by Fern, tracks learned operational patterns
5. `search_tools` works — keyword search over tool registry, returns relevant matches
6. Two-tier tool system works — built-ins always available, dynamic tools discovered via search
7. `improve_tool` works — sends existing def + feedback to Claude, saves improved version
8. `delete_tool` works — removes dynamic tools from registry and disk, rejects built-ins
9. Orchestrator prompt teaches Fern about all new capabilities
10. Nightly consolidation has awareness of behaviors.md for context
11. Full self-improving loop works: create tool → use → improve → learn behavior
12. Max 10 tool calls per message
13. All files persist across restarts (personality, behaviors, memory, tools)
14. Graceful degradation: without Anthropic API key, search/delete/read still work, just no improve/create

---

## Phase 5 preview (what's next)

- **Planning stage**: Fern breaks complex requests into multi-step plans before executing
- **Reflection logging**: SQLite table tracking tool success/failure rates over time
- **User-provided API keys**: Fern asks for and securely stores API keys for premium services
- **MCP server**: expose Fern's tool registry as an MCP server
- **Multi-user awareness**: different memory/behaviors per user
- **Claude Code integration**: Fern can modify its own Rust source for deep self-modification
