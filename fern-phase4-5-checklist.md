# Fern Phase 4.5: Signal Adapter (Drop Matrix)

> **Goal**: Replace Matrix/Conduit with a direct Signal connection via signal-cli-rest-api. Fern becomes a Signal bot you text directly — no bridge, no homeserver, no Element. The messaging layer becomes a swappable trait so the project can support any adapter (Signal, Matrix, Discord, CLI, etc.) by implementing one interface.
>
> **Why**: Matrix was infrastructure overhead for a personal AI. Three services (Conduit + bridge + Fern) to send one text message. Signal is Jason's daily driver. This makes Fern feel like texting a friend, not operating a homelab.
>
> **Architecture**:
> ```
> You on Signal
>     ↕
> signal-cli-rest-api (Docker, json-rpc mode)
>     ↕ WebSocket (receive) + HTTP POST (send)
> Fern (Rust, SignalAdapter)
>     ↓
> Orchestrator → Tools → Claude/Cerebras (unchanged)
> ```
>
> **What changes**:
> - `bot.rs` (matrix-sdk client) → replaced by adapter trait + signal implementation
> - `docker-compose.yml` → drop Conduit, add signal-cli-rest-api
> - `Cargo.toml` → drop matrix-sdk, add tokio-tungstenite for WebSocket
>
> **What stays the same**:
> - Orchestrator, tools, memory, personality, behaviors — everything from Phase 1-4
> - Cerebras + Anthropic clients
> - SQLite database, message persistence
> - Docker deployment on Hetzner VPS
>
> **New crates**: `tokio-tungstenite` (WebSocket client), `futures-util` (stream helpers). Drop `matrix-sdk`.
>
> **Rule**: Complete every checkbox in order. Tests first, then implementation.

---

## 4.5.1 — Messaging adapter trait

The core abstraction: a trait that any messaging backend implements. This decouples Fern's brain from how messages arrive and leave.

- [ ] Create `src/adapter/mod.rs`
- [ ] Define the `MessagingAdapter` trait:
  ```rust
  #[async_trait]
  pub trait MessagingAdapter: Send + Sync {
      /// Start listening for incoming messages. Calls `handler` for each message.
      /// This should run forever (blocking the task).
      async fn run(&self, handler: Arc<dyn MessageHandler>) -> Result<(), String>;

      /// Send a message to a conversation.
      async fn send_message(&self, conversation_id: &str, text: &str) -> Result<(), String>;
  }

  #[async_trait]
  pub trait MessageHandler: Send + Sync {
      /// Called when a message arrives. Returns the response text.
      async fn handle_message(
          &self,
          sender_id: &str,
          conversation_id: &str,
          text: &str,
      ) -> Result<String, String>;
  }
  ```
- [ ] The `MessageHandler` trait is what the orchestrator implements — it receives a message and returns a response. The adapter calls `handle_message` and then sends the returned text via `send_message`.
- [ ] Note: `sender_id` and `conversation_id` are abstract strings. For Signal, sender_id is a phone number like `+15551234567` and conversation_id is the same (for DMs) or a group ID.
- [ ] **TEST FIRST**:
  - Test: `mock_adapter_sends_and_receives` — create a mock adapter and mock handler, verify the trait compiles and can be used with Arc
  - Test: `handler_returns_response` — mock handler returns "hello", assert the string is returned
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.5.2 — Orchestrator as MessageHandler

The orchestrator currently gets called directly from `bot.rs` with a `send_fn` callback for interim messages. Refactor it to implement `MessageHandler`, and give it a reference to the adapter for sending interim messages.

- [ ] Create `src/adapter/orchestrator_handler.rs`
- [ ] Implement a `FernHandler` struct:
  ```rust
  pub struct FernHandler {
      orchestrator: Arc<Orchestrator>,
      adapter: Arc<dyn MessagingAdapter>,
      data_dir: String,
      db: SqlitePool,
  }
  ```
- [ ] Implement `MessageHandler` for `FernHandler`:
  - `handle_message()`:
    1. Check for `/reset` command — handle directly (reset memory, clear messages, return confirmation)
    2. Check for `/tools` command — handle directly (list tools, return formatted list)
    3. For everything else, build a `send_fn` that calls `self.adapter.send_message(conversation_id, text)`
    4. Call `self.orchestrator.process_message(sender_id, conversation_id, text, send_fn).await`
    5. Return the final response text
- [ ] This moves the `/reset` and `/tools` logic out of the old `bot.rs` into a reusable handler
- [ ] **TEST FIRST**:
  - Test: `handler_reset_command` — send "/reset", assert memory reset and response contains "reset"
  - Test: `handler_tools_command` — send "/tools", assert response lists registered tools
  - Test: `handler_normal_message` — send a regular message, mock orchestrator, assert orchestrator was called
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.5.3 — Signal adapter: sending messages

Implement the send side first — it's simpler (just HTTP POST).

- [ ] Create `src/adapter/signal.rs`
- [ ] Define `SignalAdapter` struct:
  ```rust
  pub struct SignalAdapter {
      api_url: String,          // e.g. "http://signal-api:8080"
      account_number: String,   // e.g. "+15551234567" (Fern's Signal number)
      http: reqwest::Client,
  }
  ```
- [ ] Add config fields: `SIGNAL_API_URL` (required), `SIGNAL_ACCOUNT_NUMBER` (required)
- [ ] Implement `send_message()`:
  - POST to `{api_url}/v2/send`
  - Body:
    ```json
    {
      "message": "text here",
      "number": "{account_number}",
      "recipients": ["{conversation_id}"]
    }
    ```
  - Content-Type: application/json
  - Handle errors (API down, invalid number, etc.)
  - For long messages, split at 2000 chars (Signal has a limit around 6000 but keep it readable)
- [ ] **TEST FIRST**:
  - Test: `send_message_posts_correctly` — use wiremock, assert POST to /v2/send with correct body
  - Test: `send_message_includes_recipient` — assert conversation_id appears in recipients array
  - Test: `send_message_handles_api_error` — mock 500 response, assert error returned gracefully
  - Test: `send_message_splits_long_text` — send 5000 char message, assert multiple POSTs made
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.5.4 — Signal adapter: receiving messages via WebSocket

In json-rpc mode, signal-cli-rest-api exposes a WebSocket at `/v1/receive/{number}` that pushes incoming messages in real-time.

- [ ] Add `tokio-tungstenite` and `futures-util` to Cargo.toml
- [ ] Implement `run()` on `SignalAdapter`:
  - Connect WebSocket to `ws://{api_url}/v1/receive/{account_number}`
    - Replace `http://` with `ws://` in the URL
  - Listen for incoming frames in a loop
  - Parse each frame as JSON — the envelope format is:
    ```json
    {
      "envelope": {
        "sourceNumber": "+15559876543",
        "sourceName": "Jason",
        "dataMessage": {
          "message": "hey fern whats up",
          "timestamp": 1234567890
        }
      }
    }
    ```
  - Extract `sourceNumber` as sender_id
  - Extract `dataMessage.message` as the text
  - Use `sourceNumber` as conversation_id for DMs
  - Skip messages where `dataMessage` is null (typing indicators, read receipts, etc.)
  - Skip messages where `sourceNumber` == `account_number` (Fern's own messages / sync messages)
  - Call `handler.handle_message(sender_id, conversation_id, text).await`
  - Send the response via `self.send_message(conversation_id, response).await`
  - On WebSocket disconnect: log, wait 5 seconds, reconnect (infinite retry loop)
  - On parse errors: log and skip, don't crash
- [ ] Handle group messages (stretch goal — can be a later PR):
  - Group messages have `dataMessage.groupInfo.groupId` instead of just sourceNumber
  - For now, just handle DMs (1:1 conversations)
- [ ] **TEST FIRST**:
  - Test: `parse_signal_envelope` — parse a sample JSON envelope, extract sender + message
  - Test: `parse_envelope_skips_typing` — envelope with no dataMessage, assert skipped
  - Test: `parse_envelope_skips_self` — sourceNumber matches account, assert skipped
  - Test: `parse_envelope_skips_empty_message` — dataMessage exists but message is null, assert skipped
  - (Full WebSocket integration test is hard to unit test — manual testing in 4.5.7)
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.5.5 — Update Cargo.toml and remove Matrix dependencies

- [ ] Add to Cargo.toml:
  ```toml
  tokio-tungstenite = { version = "0.21", features = ["connect"] }
  futures-util = "0.3"
  ```
- [ ] Remove from Cargo.toml:
  ```toml
  matrix-sdk = ...  # goodbye
  ```
- [ ] Remove `src/bot.rs` entirely
- [ ] Remove any `matrix_sdk` imports from `main.rs` and other files
- [ ] Update `src/config.rs`:
  - Remove: `MATRIX_HOMESERVER`, `MATRIX_USER`, `MATRIX_PASSWORD`
  - Add: `SIGNAL_API_URL`, `SIGNAL_ACCOUNT_NUMBER`
- [ ] Update `src/tools/remind.rs`:
  - The reminder loop currently uses `matrix_sdk::Client` to send reminder messages
  - Change it to accept `Arc<dyn MessagingAdapter>` instead
  - When a reminder fires, call `adapter.send_message(room_id, text).await`
  - The `room_id` stored in reminders becomes `conversation_id` (Signal phone number)
- [ ] `cargo build` must succeed with zero matrix-sdk references
- [ ] **TEST FIRST**:
  - Test: `config_loads_signal_fields` — assert SIGNAL_API_URL and SIGNAL_ACCOUNT_NUMBER load from env
  - Test: `reminder_fires_via_adapter` — mock adapter, set reminder, trigger fire, assert adapter.send_message called
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 4.5.6 — Wire everything in main.rs

- [ ] Update `main.rs` to use the adapter pattern:
  1. Init tracing
  2. Load config (now with SIGNAL_API_URL, SIGNAL_ACCOUNT_NUMBER)
  3. Init database
  4. Create Cerebras client
  5. Create Anthropic client (optional)
  6. Build ToolRegistry (same as Phase 4, all tools)
  7. Create Orchestrator
  8. Create SignalAdapter
  9. Create FernHandler (wraps orchestrator + adapter ref)
  10. Spawn nightly consolidation loop
  11. Spawn reminder loop (now takes `Arc<dyn MessagingAdapter>`)
  12. Run `adapter.run(handler)` — this blocks forever, listening for Signal messages
- [ ] The key difference: no more Matrix login flow, no SSO, no room joining. Just connect WebSocket and go.
- [ ] `cargo build` succeeds
- [ ] `cargo test` — ALL tests pass (phase 1 DB tests + phase 2/3/4 tool tests + new adapter tests)
- [ ] `cargo clippy` + `cargo fmt` clean

## 4.5.7 — Docker and deployment

- [ ] Update `docker-compose.yml`:
  ```yaml
  services:
    signal-api:
      image: bbernhard/signal-cli-rest-api:latest
      container_name: signal-api
      restart: unless-stopped
      environment:
        - MODE=json-rpc
      ports:
        - "127.0.0.1:8080:8080"   # only localhost, not exposed
      volumes:
        - ./signal-cli-config:/home/.local/share/signal-cli

    fern:
      build: ./app
      container_name: fern
      restart: unless-stopped
      depends_on:
        - signal-api
      environment:
        - SIGNAL_API_URL=http://signal-api:8080
        - SIGNAL_ACCOUNT_NUMBER=${SIGNAL_ACCOUNT_NUMBER}
        - CEREBRAS_API_KEY=${CEREBRAS_API_KEY}
        - CEREBRAS_MODEL=${CEREBRAS_MODEL}
        - ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
        - DATA_DIR=/data
        - RUST_LOG=fern=debug
      volumes:
        - fern_data:/data

  volumes:
    fern_data:
  ```
- [ ] Remove Conduit from docker-compose entirely
- [ ] Remove Caddy config for Matrix federation (unless you want to keep kcodes.me for other things)
- [ ] Link Signal account (one-time setup, must be done in `normal` mode first):
  ```bash
  # Step 1: Start signal-api in normal mode temporarily
  docker run --rm -p 8080:8080 \
    -v $(pwd)/signal-cli-config:/home/.local/share/signal-cli \
    -e MODE=normal \
    bbernhard/signal-cli-rest-api:latest

  # Step 2: Open browser to http://your-vps-ip:8080/v1/qrcodelink?device_name=fern
  # Step 3: Scan QR code with Signal app → Settings → Linked Devices
  # Step 4: Ctrl+C to stop the temp container
  # Step 5: Now start normally with json-rpc mode via docker compose
  ```
- [ ] Update `.env` on VPS:
  ```bash
  SIGNAL_ACCOUNT_NUMBER=+1XXXXXXXXXX  # your Signal number
  CEREBRAS_API_KEY=...
  CEREBRAS_MODEL=llama3.1-8b
  ANTHROPIC_API_KEY=...
  ```
- [ ] Deploy:
  ```bash
  cd /opt/fern/app && git pull
  cd /opt/fern && docker compose build --no-cache fern
  docker compose up -d
  docker compose logs fern --since 1m
  docker compose logs signal-api --since 1m
  ```

## 4.5.8 — Manual test

- [ ] Send a Signal text to your linked number: "hey fern"
  - Fern should respond via Signal
- [ ] Test tools: "what time is it?"
  - Should use current_time tool, respond with time
- [ ] Test memory: "my name is jason and i like rust"
  - Then: "what do you know about me?"
- [ ] Test tool creation: "what's the weather in austin?"
  - Should create a tool via Claude, use it, respond
- [ ] Test reminders: "remind me in 1 minute to stretch"
  - Should confirm, then send reminder via Signal 1 minute later
- [ ] Test personality: "you should be more sarcastic"
  - Should update personality.md
- [ ] Test /reset: send "/reset"
  - Should reset memory, confirm
- [ ] Restart fern: `docker compose restart fern`
  - Send another message — should reconnect WebSocket and work
- [ ] Check logs:
  ```bash
  docker compose logs fern --since 10m
  docker compose logs signal-api --since 10m
  ```

## 4.5.9 — Cleanup & commit

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo fmt` — formatted
- [ ] No matrix-sdk references anywhere in codebase
- [ ] No hardcoded phone numbers or secrets
- [ ] `git add -A && git commit -m "phase 4.5: signal adapter, drop matrix"`
- [ ] Tag: `git tag v0.6.0-signal`

---

## Phase 4.5 completion criteria

1. `cargo test` passes all tests
2. `cargo clippy` and `cargo fmt` clean
3. Matrix SDK fully removed from codebase
4. `MessagingAdapter` trait exists and is generic
5. `SignalAdapter` implements the trait — sends via HTTP POST, receives via WebSocket
6. `FernHandler` implements `MessageHandler` and routes to orchestrator
7. Reminder loop uses adapter trait, not matrix client
8. All Phase 2/3/4 features still work (tools, memory, personality, behaviors, tool search, tool improvement)
9. Docker compose has signal-api + fern only (no Conduit)
10. Signal messages flow end-to-end: text → Fern → response
11. WebSocket reconnects on disconnect
12. Parse errors don't crash the bot

---

## Notes for forking

The `MessagingAdapter` trait makes it straightforward to add other backends:

```
src/adapter/
  mod.rs          — trait definitions
  signal.rs       — Signal via signal-cli-rest-api
  # Future adapters someone could write:
  # discord.rs    — Discord via serenity
  # telegram.rs   — Telegram via teloxide
  # cli.rs        — stdin/stdout for local testing
  # matrix.rs     — re-add Matrix if someone wants it
```

To add a new adapter: implement `MessagingAdapter`, update `main.rs` to instantiate it based on config, done. The orchestrator doesn't know or care what transport delivers the messages.
