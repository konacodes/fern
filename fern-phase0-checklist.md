# Fern Phase 0: Echo Bot (Rust + Matrix)

> **Goal**: Fern connects to a Conduit homeserver, listens for messages, echoes them back with a "🌿 " prefix. TDD throughout.
>
> **Stack**: Rust, matrix-rust-sdk, Conduit (homeserver), Docker, SQLite
>
> **Rule**: Do NOT skip ahead to Phase 1. Complete every checkbox below first.

---

## 0.1 — Project scaffold

- [ ] `cargo init`
- [ ] Set up `Cargo.toml`:
  - edition = "2021", rust-version = "1.75"
  - `[dependencies]`: matrix-sdk, tokio (full features), serde + serde_json, tracing + tracing-subscriber, dotenvy, rusqlite (bundled feature)
  - `[dev-dependencies]`: tokio-test, wiremock (for HTTP mocking)
- [ ] Create directory layout:
  ```
  fern/
  ├── src/
  │   ├── main.rs          # entrypoint
  │   ├── config.rs         # env var loading
  │   ├── bot.rs            # matrix client + message handler
  │   └── lib.rs            # re-exports for test access
  ├── tests/
  │   └── echo_test.rs      # integration tests
  ├── Cargo.toml
  ├── .env.example
  └── .gitignore
  ```
- [ ] `.gitignore`: `/target`, `.env`, `*.db`, `data/`
- [ ] `cargo build` compiles with zero warnings
- [ ] `cargo clippy` passes with zero warnings
- [ ] `cargo fmt -- --check` passes

## 0.2 — Config

- [ ] `src/config.rs`: define a `Config` struct with fields:
  - `homeserver_url: String` (e.g. `http://localhost:6167`)
  - `bot_user: String` (e.g. `@fern:yourdomain.tld`)
  - `bot_password: String`
  - `data_dir: String` (default: `./data`, for matrix-sdk store + sqlite)
- [ ] Load from environment using `dotenvy` + `std::env::var`
  - Panic early with clear error message if any required var is missing
- [ ] `.env.example` with placeholder values and comments explaining each
- [ ] **TEST**: unit test that missing env vars produce the expected panic/error message

## 0.3 — Matrix client setup

- [ ] `src/bot.rs`: define `FernBot` struct holding:
  - `client: matrix_sdk::Client`
  - `config: Config`
- [ ] Implement `FernBot::new(config: Config) -> Result<Self>`:
  - Build client with `Client::builder().homeserver_url(...).sqlite_store(...)` 
  - Login with `client.matrix_auth().login_username(&config.bot_user, &config.bot_password)`
  - Log successful login at `tracing::info!` level
- [ ] Implement `FernBot::run(&self) -> Result<()>`:
  - Register event handler for `SyncRoomMessageEvent`
  - Call `client.sync(SyncSettings::default())` to block and listen
- [ ] **TEST**: write a test that `FernBot::new` fails gracefully with a bad homeserver URL (should return Err, not panic)

## 0.4 — Echo handler (write tests FIRST)

- [ ] **TEST FIRST** — in `tests/echo_test.rs` or `src/bot.rs` `#[cfg(test)]` module:
  - Test: `echo_format` — given input text "hello", returns "🌿 hello"
  - Test: `echo_format_empty` — given empty string "", returns "🌿 "
  - Test: `echo_format_unicode` — given "こんにちは", returns "🌿 こんにちは"
  - Test: `echo_ignores_own_messages` — if sender == bot user ID, return None (don't echo self)
  - Test: `echo_ignores_non_text` — image/video/file messages return None
  - All tests should FAIL initially (red phase)
- [ ] Extract echo logic into a pure function:
  ```rust
  pub fn format_echo(text: &str) -> String {
      format!("🌿 {text}")
  }
  
  pub fn should_echo(sender: &UserId, own_id: &UserId, msg: &MessageType) -> Option<String> {
      // Return Some(formatted) if we should respond, None otherwise
  }
  ```
- [ ] Make all tests pass (green phase)
- [ ] Wire `should_echo` into the actual event handler in `FernBot::run`:
  - On receiving `SyncRoomMessageEvent`, extract sender + message type
  - If `should_echo` returns `Some(response)`, send it to the room via `room.send(...)`
  - If None, do nothing
- [ ] `cargo clippy` + `cargo fmt` pass

## 0.5 — Main entrypoint

- [ ] `src/main.rs`:
  - Initialize `tracing_subscriber` with env filter (default `info`)
  - Load config via `Config::from_env()`
  - Create `FernBot::new(config).await?`
  - Call `bot.run().await?`
  - Handle errors with clear log messages, exit code 1
- [ ] Confirm `cargo run` starts, attempts to connect (will fail without homeserver, that's fine)
- [ ] `cargo test` — all unit tests pass
- [ ] `cargo clippy` + `cargo fmt` — clean

## 0.6 — Conduit homeserver (Docker)

- [ ] Create `conduit/conduit.toml`:
  - `server_name` = your domain (e.g. `fern.local` for dev)
  - `database_backend` = "rocksdb"
  - `port` = 6167
  - `allow_registration` = true (for initial bot account creation, disable after)
  - `max_request_size` = 20_000_000 (20MB)
  - `trusted_servers` = ["matrix.org"] (for federation, optional)
- [ ] Create `docker-compose.yml`:
  ```yaml
  services:
    conduit:
      image: matrixconduit/matrix-conduit:latest
      volumes:
        - conduit_data:/var/lib/matrix-conduit
        - ./conduit/conduit.toml:/etc/matrix-conduit/conduit.toml
      ports:
        - "127.0.0.1:6167:6167"
      restart: unless-stopped
  
    fern:
      build: .
      depends_on:
        - conduit
      env_file: .env
      volumes:
        - fern_data:/app/data
      restart: unless-stopped
  
  volumes:
    conduit_data:
    fern_data:
  ```
- [ ] Create `Dockerfile` for Fern:
  - Multi-stage build: `rust:1.75-slim` builder → `debian:bookworm-slim` runtime
  - Builder: `cargo build --release`
  - Runtime: copy binary, create non-root user, set `RUST_LOG=info`
  - Entrypoint: `./fern`
- [ ] `docker compose up conduit` — confirm Conduit starts and is reachable at `localhost:6167`
- [ ] Register bot account on Conduit (via Element or curl to the registration endpoint)
- [ ] Create a test room, invite the bot
- [ ] `docker compose up` — confirm Fern starts, logs in, joins the room

## 0.7 — End-to-end manual test

- [ ] Open Element (web/desktop/mobile) and log in to your Conduit homeserver
- [ ] Send a message to the room Fern is in
- [ ] Confirm Fern echoes back with "🌿 " prefix
- [ ] Send a unicode message — confirm echo works
- [ ] Send an image — confirm Fern does NOT echo (ignores non-text)
- [ ] Check Fern's logs — confirm tracing output shows received/sent messages
- [ ] Send 5+ messages rapidly — confirm all are echoed, none dropped

## 0.8 — Cleanup & commit

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo fmt` — formatted
- [ ] Remove any TODO/FIXME/hardcoded values
- [ ] Ensure `.env` is gitignored, `.env.example` has all vars documented
- [ ] `git init && git add -A && git commit -m "phase 0: echo bot on matrix"`
- [ ] Tag: `git tag v0.1.0-echo`

---

## Phase 0 completion criteria

All of the following must be true before moving to Phase 1:

1. `cargo test` passes with all echo logic tests green
2. `cargo clippy` and `cargo fmt` report zero issues
3. `docker compose up` starts both Conduit and Fern
4. Sending a text message via Element produces an echo with "🌿 " prefix
5. Non-text messages are silently ignored
6. Bot does not echo its own messages
7. Logs show clean tracing output for each message received and sent
