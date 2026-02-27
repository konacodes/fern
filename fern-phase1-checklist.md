# Fern Phase 1: Basic Conversations (Rust + Matrix + Cerebras)

> **Goal**: Fern responds with AI-generated messages instead of echoing. Uses Cerebras Qwen3 235B for conversation, persists messages to SQLite, splits long responses for readability.
>
> **New crates**: reqwest, sqlx (sqlite feature), chrono, uuid
>
> **Rule**: Do NOT skip ahead to Phase 2. Complete every checkbox below first.

---

## 1.1 — Add dependencies

- [ ] Add to `Cargo.toml` `[dependencies]`:
  - `reqwest = { version = "0.12", features = ["json"] }`
  - `sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }`
  - `chrono = { version = "0.4", features = ["serde"] }`
  - `uuid = { version = "1", features = ["v4"] }`
- [ ] Add new source files to the project structure:
  ```
  src/
  ├── main.rs
  ├── config.rs         # updated with new env vars
  ├── bot.rs            # updated to use ConversationEngine
  ├── lib.rs
  ├── ai/
  │   ├── mod.rs
  │   └── cerebras.rs   # Cerebras API client
  ├── engine/
  │   ├── mod.rs
  │   └── conversation.rs  # conversation engine + personality
  ├── db/
  │   ├── mod.rs
  │   └── messages.rs   # message persistence
  └── sender.rs         # ChainSender for splitting long messages
  ```
- [ ] `cargo build` compiles
- [ ] `cargo clippy` passes

## 1.2 — Config updates

- [ ] Add new fields to `Config`:
  - `cerebras_api_key: String`
  - `cerebras_model: String` (default: `"qwen-3-235b"`)
  - `cerebras_base_url: String` (default: `"https://api.cerebras.ai/v1"`)
  - `database_url: String` (default: `"sqlite://{data_dir}/fern.db"`)
- [ ] Update `.env.example` with new vars and comments
- [ ] **TEST**: missing `CEREBRAS_API_KEY` produces clear error

## 1.3 — SQLite database setup

- [ ] Create `src/db/mod.rs`:
  - `pub async fn init_db(database_url: &str) -> Result<SqlitePool>`
  - Run migrations on startup (embedded with `sqlx::migrate!` or raw SQL)
- [ ] Create migration SQL (either `migrations/001_init.sql` or embedded string):
  ```sql
  CREATE TABLE IF NOT EXISTS users (
      id TEXT PRIMARY KEY,              -- Matrix user ID e.g. @jason:kcodes.me
      display_name TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  );

  CREATE TABLE IF NOT EXISTS messages (
      id TEXT PRIMARY KEY,              -- UUID
      user_id TEXT NOT NULL,
      room_id TEXT NOT NULL,
      role TEXT NOT NULL,               -- 'user' or 'assistant'
      content TEXT NOT NULL,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      FOREIGN KEY (user_id) REFERENCES users(id)
  );

  CREATE INDEX IF NOT EXISTS idx_messages_room_created
  ON messages(room_id, created_at);
  ```
- [ ] Create `src/db/messages.rs`:
  - `pub async fn upsert_user(pool: &SqlitePool, user_id: &str, display_name: Option<&str>) -> Result<()>`
  - `pub async fn save_message(pool: &SqlitePool, user_id: &str, room_id: &str, role: &str, content: &str) -> Result<()>`
  - `pub async fn get_recent_messages(pool: &SqlitePool, room_id: &str, limit: i64) -> Result<Vec<StoredMessage>>`
  - Define `StoredMessage` struct: `{ id, user_id, room_id, role, content, created_at }`
- [ ] **TEST FIRST**:
  - Test: `save_and_retrieve_messages` — save 3 messages, retrieve with limit 10, assert all 3 returned in order
  - Test: `recent_messages_limit` — save 5 messages, retrieve with limit 2, assert only last 2 returned
  - Test: `upsert_user_creates_and_updates` — upsert twice with different display name, assert updated
  - Use in-memory SQLite (`sqlite::memory:`) for tests
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 1.4 — Cerebras API client

- [ ] Create `src/ai/cerebras.rs`:
  - Define `CerebrasClient` struct holding `reqwest::Client`, `api_key`, `base_url`, `model`
  - Define `ChatMessage` struct: `{ role: String, content: String }`
  - Implement `CerebrasClient::new(config: &Config) -> Self`
  - Implement `pub async fn chat(&self, system: &str, messages: Vec<ChatMessage>) -> Result<String>`:
    - POST to `{base_url}/chat/completions`
    - Headers: `Authorization: Bearer {api_key}`, `Content-Type: application/json`
    - Body: `{ "model": model, "messages": [{"role":"system","content":system}, ...messages], "max_tokens": 512, "temperature": 0.7 }`
    - Parse response: extract `choices[0].message.content`
    - Return the text content
    - On HTTP error or parse failure, return descriptive error
- [ ] **TEST FIRST**:
  - Test: `cerebras_request_format` — use `wiremock` to mock the API endpoint. Assert the request body contains correct model, system message, and user messages in the right format
  - Test: `cerebras_parses_response` — mock a successful response, assert the returned string matches `choices[0].message.content`
  - Test: `cerebras_handles_error` — mock a 500 response, assert `chat()` returns Err with descriptive message
  - Test: `cerebras_handles_malformed_json` — mock a 200 with garbage body, assert Err
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 1.5 — Fern's personality prompt

- [ ] Create `src/engine/conversation.rs`
- [ ] Define `FERN_SYSTEM_PROMPT` as a const string:
  ```
  you're fern 🌿 — a warm, witty personal assistant who lives in your texts.

  personality:
  - lowercase casual. no periods at end of messages unless it's dramatic
  - brief — 1-3 sentences usually. you're texting, not writing essays
  - warm but not sycophantic. you care genuinely
  - playful when the moment calls for it
  - you remember what people tell you (when memory is available)
  - you admit when you don't know something

  rules:
  - never start with "Hey!" or "Hi there!" — just dive in
  - never use bullet points or markdown formatting in messages
  - never say "as an AI" or "I'm just a language model"
  - keep responses under 300 characters when possible
  - if a response needs to be longer, that's fine, but prefer brevity
  - use emoji sparingly — 🌿 is your signature but don't overdo it
  ```
- [ ] This is just a const — no tests needed, but verify it compiles

## 1.6 — Conversation engine

- [ ] Define `ConversationEngine` struct holding:
  - `cerebras: CerebrasClient`
  - `db: SqlitePool`
- [ ] Implement `ConversationEngine::new(cerebras: CerebrasClient, db: SqlitePool) -> Self`
- [ ] Implement `pub async fn respond(&self, user_id: &str, room_id: &str, message: &str) -> Result<String>`:
  1. `upsert_user(pool, user_id, None)`
  2. `save_message(pool, user_id, room_id, "user", message)`
  3. `get_recent_messages(pool, room_id, 30)` — last 30 messages for context
  4. Convert stored messages to `Vec<ChatMessage>` (role + content)
  5. Call `cerebras.chat(FERN_SYSTEM_PROMPT, messages)`
  6. `save_message(pool, user_id, room_id, "assistant", &response)`
  7. Return the response
  8. On AI error, return a friendly fallback like `"hmm something went wrong on my end, give me a sec 🌿"`
- [ ] **TEST FIRST**:
  - Test: `respond_saves_user_and_messages` — use wiremock for Cerebras, in-memory SQLite. Call `respond`, assert user was created, both user message and assistant response are in DB
  - Test: `respond_includes_history` — save some messages to DB first, call `respond`, inspect the wiremock request body to verify history was included
  - Test: `respond_handles_ai_failure` — mock Cerebras returning 500, assert `respond` returns the friendly fallback string (not an Err)
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 1.7 — ChainSender for long messages

- [ ] Create `src/sender.rs`:
  - Define `pub fn split_message(text: &str, max_len: usize) -> Vec<String>`:
    - Default `max_len`: 500 characters (comfortable for Matrix, not SMS but keeps messages readable)
    - Split on paragraph breaks (`\n\n`) first
    - If a paragraph is still too long, split on sentence boundaries (`. `)
    - If a sentence is still too long, split on word boundaries
    - Never split mid-word
    - Trim whitespace on each chunk
    - Filter out empty chunks
- [ ] **TEST FIRST**:
  - Test: `short_message_no_split` — "hello" → vec!["hello"]
  - Test: `split_on_paragraphs` — two paragraphs under max_len each → vec![para1, para2]
  - Test: `split_long_paragraph_on_sentences` — one paragraph with 3 sentences totaling over max_len → splits at sentence boundaries
  - Test: `split_on_words_as_fallback` — single very long word-sequence with no periods → splits on word boundaries
  - Test: `no_empty_chunks` — text with multiple newlines → no empty strings in result
  - Test: `unicode_safe` — Japanese/emoji text splits correctly without breaking characters
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 1.8 — Wire everything into bot.rs

- [ ] Update `FernBot` struct to hold:
  - `client: matrix_sdk::Client`
  - `engine: Arc<ConversationEngine>` (Arc because the event handler closure needs ownership)
- [ ] Update `FernBot::new`:
  1. Load config
  2. Init database: `db::init_db(&config.database_url).await?`
  3. Create `CerebrasClient::new(&config)`
  4. Create `ConversationEngine::new(cerebras, db)`
  5. Build Matrix client and login (same as before)
- [ ] Update the message event handler:
  - Replace the echo logic with:
    1. Extract sender, room_id, message text
    2. Skip if sender is own user ID (same as before)
    3. Skip if message type is not text (same as before)
    4. Call `engine.respond(sender, room_id, text).await`
    5. Split response with `split_message(response, 500)`
    6. For each chunk, send to room with 500ms delay between chunks (`tokio::time::sleep`)
- [ ] Add invite auto-accept handler:
  - Register handler for `StrippedRoomMemberEvent`
  - If the invite is for the bot's own user ID, call `room.join().await`
  - Log the invite acceptance at `tracing::info!`
- [ ] Remove old `format_echo` / `should_echo` from the event handler (keep the functions in code for now, they're tested)
- [ ] `cargo clippy` passes

## 1.9 — Update main.rs

- [ ] Update `main.rs` to construct everything in order:
  1. Init tracing
  2. Load config
  3. Init database
  4. Create CerebrasClient
  5. Create ConversationEngine
  6. Create FernBot with engine
  7. Run bot
- [ ] `cargo build` succeeds
- [ ] `cargo test` — all tests pass (echo tests + new tests)
- [ ] `cargo clippy` + `cargo fmt` clean

## 1.10 — Deploy and manual test

- [ ] Update `.env` on VPS with `CEREBRAS_API_KEY` and `CEREBRAS_MODEL`
- [ ] Copy updated code to VPS
- [ ] `docker compose up -d --build fern`
- [ ] Open Element, send "hey" — Fern should respond conversationally, not echo
- [ ] Send "what's your name?" — Fern should identify as fern
- [ ] Send 5 messages in a row — verify Fern remembers context from earlier in the conversation
- [ ] Send a very long question — verify response is split into multiple messages
- [ ] Check logs: `docker compose logs fern | tail -20` — clean tracing, no errors
- [ ] Verify SQLite has data:
  ```bash
  docker compose exec fern sqlite3 /data/fern.db "SELECT role, content FROM messages ORDER BY created_at DESC LIMIT 10;"
  ```

## 1.11 — Cleanup & commit

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo fmt` — formatted
- [ ] No hardcoded API keys, URLs, or secrets anywhere in source
- [ ] `git add -A && git commit -m "phase 1: AI conversations via cerebras"`
- [ ] Tag: `git tag v0.2.0-chat`

---

## Phase 1 completion criteria

All of the following must be true before moving to Phase 2:

1. `cargo test` passes with all tests green (echo tests + DB + Cerebras + engine + sender)
2. `cargo clippy` and `cargo fmt` report zero issues
3. Fern responds conversationally using Cerebras Qwen3 235B
4. Messages are persisted to SQLite (both user and assistant messages)
5. Conversation history (last 30 messages) is included in AI context
6. Long responses are split into multiple Matrix messages
7. Fern auto-accepts room invites
8. Fern's personality matches the system prompt (casual, brief, lowercase)
9. AI failures produce a friendly fallback message, not a crash
