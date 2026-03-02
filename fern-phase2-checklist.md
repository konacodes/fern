# Fern Phase 2: Long-Term Memory (Nightly Consolidation)

> **Goal**: Every night at midnight, Fern reviews the day's conversations and updates a structured `memory.md` file. Before each response, Fern reads `memory.md` and injects it into the system prompt. The result is a curated, evolving understanding of the user — not a pile of atomic facts.
>
> **Architecture**: A `tokio` background task runs a cron-style loop. At midnight (local time), it gathers all messages from the past 24h, sends them plus the current `memory.md` to Cerebras, and writes the updated file. On every message, Fern reads the file and includes it in context.
>
> **New crates**: none (reuse existing: tokio, chrono, Arc, sqlx)
>
> **Rule**: Do NOT skip ahead. Complete every checkbox in order.

---

## 2.1 — memory.md format and seed file

- [ ] Create `src/memory/mod.rs`
- [ ] Define `MEMORY_TEMPLATE` as a const — the initial empty memory.md:
  ```markdown
  # Fern's Memory

  ## Working Memory
  _nothing right now_

  ## Projects & Work
  _nothing yet_

  ## Preferences & Style
  _nothing yet_

  ## Long-Term Memory
  _nothing yet_
  ```
- [ ] Define `MEMORY_FILENAME` as `"memory.md"`
- [ ] Implement `pub fn memory_path(data_dir: &str) -> PathBuf` — returns `{data_dir}/memory.md`
- [ ] Implement `pub fn read_memory(data_dir: &str) -> String`:
  - If file exists, read and return contents
  - If file doesn't exist, write `MEMORY_TEMPLATE` to disk and return it
- [ ] Implement `pub fn write_memory(data_dir: &str, content: &str) -> Result<()>`:
  - Write to `{data_dir}/memory.md.tmp` first, then rename (atomic write)
- [ ] **TEST FIRST** (use a tempdir):
  - Test: `read_memory_creates_default` — fresh dir, call read_memory, assert file created with template content
  - Test: `read_memory_returns_existing` — write custom content, call read_memory, assert custom content returned
  - Test: `write_memory_atomic` — write content, read it back, assert match
  - Test: `write_memory_overwrites` — write twice with different content, assert second content persists
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.2 — Consolidation prompt

- [ ] Create `src/memory/consolidator.rs`
- [ ] Define `CONSOLIDATION_PROMPT` as a const:
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
- [ ] This is just a const — verify it compiles

## 2.3 — Consolidation engine

- [ ] Define `Consolidator` struct holding:
  - `cerebras: Arc<CerebrasClient>`
  - `db: SqlitePool`
  - `data_dir: String`
- [ ] Implement `Consolidator::new(cerebras, db, data_dir) -> Self`
- [ ] Implement `pub async fn run_consolidation(&self) -> Result<()>`:
  1. Read current memory: `read_memory(&self.data_dir)`
  2. Fetch today's messages: `get_messages_since(pool, since_datetime)` (new DB function, see below)
  3. If no messages today, log "no messages to consolidate" and return Ok
  4. Format the chat log as a readable transcript:
     ```
     [12:34] user: hey what's up
     [12:34] fern: not much, working on anything cool?
     [12:35] user: yeah building a compiler in rust
     ```
  5. Build the consolidation message:
     ```
     here's today's chat log:
     ---
     {formatted_chat}
     ---

     here's the current memory file:
     ---
     {current_memory}
     ---
     ```
  6. Call `cerebras.chat(CONSOLIDATION_PROMPT, [{role: "user", content: message}])`
  7. Validate response starts with `# Fern's Memory` (sanity check)
  8. If valid, `write_memory(&self.data_dir, &response)`
  9. If invalid, log warning and keep existing file
  10. Log summary: "consolidated N messages into memory"
- [ ] Add to `src/db/messages.rs`:
  - `pub async fn get_messages_since(pool: &SqlitePool, since: &str) -> Result<Vec<StoredMessage>>`
    - Fetches all messages with `created_at >= since`, ordered by created_at ASC
- [ ] Implement `pub fn format_chat_log(messages: &[StoredMessage]) -> String` — pure function
- [ ] **TEST FIRST**:
  - Test: `format_chat_log_basic` — 3 messages → properly formatted transcript
  - Test: `format_chat_log_empty` — no messages → empty string
  - Test: `consolidation_updates_memory` — mock Cerebras returning updated memory content, pre-populate messages in DB, call run_consolidation, assert memory.md was updated
  - Test: `consolidation_skips_when_no_messages` — empty DB, call run_consolidation, assert memory file unchanged
  - Test: `consolidation_rejects_invalid_response` — mock Cerebras returning garbage (no "# Fern's Memory" header), assert original memory preserved
  - Test: `get_messages_since_filters_correctly` — save messages with different timestamps, assert only recent ones returned
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.4 — Nightly scheduler

- [ ] Create `src/memory/scheduler.rs`
- [ ] Implement `pub async fn run_nightly_loop(consolidator: Arc<Consolidator>)`:
  - Calculate duration until next midnight (local time using chrono)
  - Loop forever:
    1. `tokio::time::sleep(duration_until_midnight)`
    2. `tracing::info!("starting nightly memory consolidation")`
    3. Call `consolidator.run_consolidation().await`
    4. Log result (success or error, never crash)
    5. Calculate next midnight (always ~24h from now, handles DST etc.)
- [ ] Implement helper: `fn duration_until_next_midnight() -> Duration`
  - Use `chrono::Local::now()` to get current local time
  - Calculate next midnight
  - Return the difference as a `tokio::time::Duration`
- [ ] **TEST FIRST**:
  - Test: `duration_until_midnight_is_positive` — assert returned duration > 0 and <= 24 hours
  - Test: `duration_until_midnight_is_less_than_24h` — assert < 24h
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.5 — Memory-aware system prompt

- [ ] In `src/engine/conversation.rs`, add:
  - `pub fn build_system_prompt(base_prompt: &str, memory_content: &str) -> String`
  - If memory is just the default template (all "_nothing yet_"), return base prompt unchanged
  - Otherwise append:
    ```
    {base_prompt}

    ---
    {memory_content}
    ---

    use these memories naturally. don't announce "i remember that..." — just let the knowledge inform your responses. if asked directly what you remember, you can share.
    ```
- [ ] **TEST FIRST**:
  - Test: `empty_memory_returns_base_prompt` — default template → base prompt only
  - Test: `memory_appended_to_prompt` — custom memory content → appears after base prompt
  - Test: `base_prompt_preserved` — memory added but base prompt is fully intact
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.6 — Wire memory into conversation engine

- [ ] Update `ConversationEngine` struct to hold:
  - `data_dir: String` (to read memory.md)
- [ ] Update `ConversationEngine::new` to accept `data_dir`
- [ ] Update `respond()`:
  1. Everything from Phase 1 stays the same, EXCEPT:
  2. **NEW**: `let memory = memory::read_memory(&self.data_dir)`
  3. **NEW**: `let prompt = build_system_prompt(FERN_SYSTEM_PROMPT, &memory)`
  4. Use `prompt` instead of `FERN_SYSTEM_PROMPT` in the Cerebras call
  5. No background extraction — that's handled by the nightly job
- [ ] Update existing engine tests to pass `data_dir` (use tempdir)
- [ ] **TEST FIRST**:
  - Test: `respond_includes_memory_in_prompt` — write a custom memory.md to tempdir, call respond, inspect wiremock request body to verify memory content appears in system prompt
  - Test: `respond_works_with_no_memory_file` — fresh tempdir (no memory.md), call respond, assert it works fine with default template
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.7 — Memory management commands

- [ ] In `src/engine/conversation.rs`, update `respond()` to intercept commands:
  - `"/memories"` → read memory.md, return it as the response directly (don't call AI)
  - `"/forget all"` → overwrite memory.md with the default template, return confirmation
  - `"/consolidate"` → trigger consolidation NOW (don't wait for midnight), return confirmation after it runs
  - For any message starting with `/`, if not recognized, pass to AI normally
- [ ] The `/consolidate` command needs access to the Consolidator — either:
  - Store `Arc<Consolidator>` in `ConversationEngine`, OR
  - Accept it as an optional dependency
- [ ] **TEST FIRST**:
  - Test: `memories_command_returns_file` — write custom memory.md, send "/memories", assert response contains the file content
  - Test: `forget_all_resets_memory` — write custom memory.md, send "/forget all", read file, assert it's the default template
  - Test: `unknown_command_passes_to_ai` — send "/blah", assert wiremock receives the AI request
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 2.8 — Wire everything in main.rs and bot.rs

- [ ] Update `main.rs`:
  1. Create `Arc<CerebrasClient>`
  2. Create `ConversationEngine` with `data_dir`
  3. Create `Consolidator`
  4. Create `Arc<Consolidator>` and pass to engine (for /consolidate command)
  5. Spawn the nightly loop: `tokio::spawn(run_nightly_loop(consolidator))`
  6. Create `FernBot` and run (same as before)
- [ ] Update `bot.rs` if any type signatures changed
- [ ] `cargo build` succeeds
- [ ] `cargo test` — ALL tests pass (phase 1 + phase 2)
- [ ] `cargo clippy` + `cargo fmt` clean

## 2.9 — Deploy and manual test

- [ ] Push code, pull on VPS, rebuild:
  ```bash
  cd /opt/fern/app && git pull
  cd /opt/fern && docker compose build --no-cache fern
  docker volume rm fern_fern_data
  docker compose up -d fern
  sleep 5
  docker compose logs fern --since 1m
  ```
- [ ] Test in Element:
  1. "my favorite language is rust and i have a wolf fursona named kona" → normal response
  2. "/memories" → should show default template (no consolidation yet)
  3. "/consolidate" → triggers manual consolidation
  4. "/memories" → should now show extracted facts from the conversation
  5. Chat more, "/consolidate" again, "/memories" → verify updates
  6. "/forget all" → resets to default
  7. Restart fern → memory.md persists (it's on the volume)
- [ ] Check that the nightly scheduler logged its next run time:
  ```bash
  docker compose logs fern | grep -i "midnight\|nightly\|consolidat"
  ```
- [ ] Verify memory.md directly:
  ```bash
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim cat /data/memory.md
  ```

## 2.10 — Cleanup & commit

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo fmt` — formatted
- [ ] No hardcoded secrets
- [ ] `git add -A && git commit -m "phase 2: nightly memory consolidation"`
- [ ] Tag: `git tag v0.3.0-memory`

---

## Phase 2 completion criteria

All of the following must be true before moving to Phase 3:

1. `cargo test` passes with all tests green (phase 1 + phase 2)
2. `cargo clippy` and `cargo fmt` report zero issues
3. `memory.md` exists on the data volume with the correct 4-section structure
4. Fern reads memory.md before every response and includes it in context
5. `/consolidate` triggers an immediate memory update from the day's messages
6. `/memories` shows the current memory file contents
7. `/forget all` resets memory to the default template
8. Nightly scheduler is running and will trigger at midnight
9. Consolidation gracefully handles: no messages, malformed AI response, empty days
10. Memory file survives container restarts
11. Memory naturally informs Fern's responses without announcing "I remember..."
