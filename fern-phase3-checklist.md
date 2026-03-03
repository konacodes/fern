# Fern Phase 3: Self-expanding tool system

> **Goal**: Fern can recognize when it lacks the tools to help, request a new tool by describing what it needs, and have Claude (Anthropic API) generate that tool. Two tool types: HTTP tools (API call templates) and script tools (Python scripts). Tools persist across restarts and are loaded on boot.
>
> **How it works**:
> 1. User asks Fern something it can't do ("what's the weather in Austin?")
> 2. Fern realizes it has no tool for this
> 3. Fern calls `request_tool` with a detailed description of what it needs
> 4. `request_tool` sends that description to Claude, which generates a tool definition
> 5. Fern validates, saves, and registers the new tool
> 6. Fern uses the new tool to answer the user's original question
> 7. The tool persists — next time anyone asks about weather, Fern just uses it
>
> **Tool types**:
> - **HTTP tools**: JSON definitions with URL templates, method, headers, body templates, response extraction. Good for APIs.
> - **Script tools**: Python scripts with a JSON manifest. Good for computation, text processing, anything local.
>
> **Models**:
> - Cerebras `gpt-oss-120b` for orchestration (tool calling)
> - Cerebras default model for consolidation and other cheap tasks
> - **Anthropic Claude** for tool generation (new dependency)
>
> **New crates**: none expected — `reqwest` and `tokio::process` already available.
>
> **Rule**: Complete every checkbox in order. Tests first, then implementation.

---

## 3.1 — Anthropic client

The tool generation system needs to talk to Claude. This is a separate client from Cerebras — simpler, since we only need basic completions with no tool calling.

- [ ] Create `src/ai/anthropic.rs`
- [ ] Define `AnthropicClient` struct:
  ```rust
  pub struct AnthropicClient {
      http: reqwest::Client,
      api_key: String,
      model: String,  // default: "claude-sonnet-4-20250514"
  }
  ```
- [ ] Add config fields: `ANTHROPIC_API_KEY` (required for phase 3, optional overall), `ANTHROPIC_MODEL` (optional, defaults to `claude-sonnet-4-20250514`)
- [ ] Implement `AnthropicClient::new(api_key, model)`
- [ ] Implement `pub async fn complete(&self, system: &str, user_message: &str) -> Result<String, ...>`:
  - POST to `https://api.anthropic.com/v1/messages`
  - Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`
  - Body: `{ model, max_tokens: 4096, system, messages: [{ role: "user", content: user_message }] }`
  - Parse response, extract `content[0].text`
- [ ] Register in `ai/mod.rs`
- [ ] **TEST FIRST**:
  - Test: `anthropic_request_format` — mock server, assert correct headers + body shape
  - Test: `anthropic_extracts_text` — mock response with `content: [{ type: "text", text: "hello" }]`, assert returns "hello"
  - Test: `anthropic_handles_error` — mock 500, assert error returned
  - Test: `anthropic_handles_malformed_json` — mock garbage body, assert error
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.2 — Dynamic tool definitions

HTTP and script tools need a common on-disk format that Fern can load on boot and write when creating new tools.

- [ ] Create `src/tools/dynamic.rs`
- [ ] Define the stored format:
  ```rust
  #[derive(Serialize, Deserialize, Clone, Debug)]
  pub struct DynamicToolDef {
      pub name: String,
      pub description: String,
      pub parameters: Vec<ToolParam>,
      pub tool_type: DynamicToolType,
  }

  #[derive(Serialize, Deserialize, Clone, Debug)]
  pub struct ToolParam {
      pub name: String,
      pub param_type: String,  // "string", "integer", "number", "boolean"
      pub description: String,
      pub required: bool,
  }

  #[derive(Serialize, Deserialize, Clone, Debug)]
  #[serde(tag = "type")]
  pub enum DynamicToolType {
      Http {
          url_template: String,       // e.g. "https://wttr.in/{{location}}?format=j1"
          method: String,             // GET, POST, etc.
          headers: HashMap<String, String>,
          body_template: Option<String>,
          response_jq: Option<String>, // jq-like path to extract from response, e.g. ".current_condition[0].temp_F"
      },
      Script {
          interpreter: String,        // "python3" or "bash"
          source: String,             // the actual script code
      },
  }
  ```
- [ ] Implement `DynamicToolDef::save(data_dir)` — writes to `{data_dir}/tools/{name}.json`
- [ ] Implement `DynamicToolDef::load(path)` — reads and deserializes a single tool file
- [ ] Implement `pub fn load_all_tools(data_dir: &str) -> Vec<DynamicToolDef>` — scans `{data_dir}/tools/*.json`, loads each, logs and skips any that fail to parse
- [ ] Implement `pub fn delete_tool(data_dir: &str, name: &str) -> Result<()>` — removes the JSON file
- [ ] **TEST FIRST** (use tempdir):
  - Test: `save_and_load_http_tool` — create an HTTP tool def, save, load, assert fields match
  - Test: `save_and_load_script_tool` — same for a script tool
  - Test: `load_all_finds_tools` — save 3 tools, load_all, assert all 3 returned
  - Test: `load_all_skips_invalid` — write a corrupt JSON file alongside valid ones, assert valid ones load and invalid is skipped
  - Test: `delete_tool_removes_file` — save, delete, assert file gone
  - Test: `tool_name_sanitization` — names with slashes or dots get rejected or sanitized
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.3 — HTTP tool executor

A generic tool that loads an HTTP definition and executes it at runtime.

- [ ] Create `src/tools/http_tool.rs`
- [ ] Define `HttpTool` struct holding a `DynamicToolDef` (must be `Http` variant) and an `reqwest::Client`
- [ ] Implement `Tool` trait:
  - `name()` / `description()` / `parameters()` — from the definition
  - `tool_schema()` — build proper OpenAI-format schema from the `parameters` vec
  - `execute(params)`:
    1. Render `url_template` by replacing `{{param_name}}` with param values (URL-encode values)
    2. Render `body_template` the same way if present
    3. Set headers from definition
    4. Make the HTTP request with a 10-second timeout
    5. If `response_jq` is set, extract that path from the JSON response (simple dot-path extraction, not full jq)
    6. If no jq path, return the raw response body (truncated to 2000 chars)
    7. On HTTP error, return a readable error message
- [ ] Implement a simple `extract_json_path(value: &serde_json::Value, path: &str) -> Option<String>` helper for dot-path extraction like `.foo.bar[0].baz`
- [ ] **TEST FIRST**:
  - Test: `http_tool_get_request` — mock server, create tool with GET template, execute, assert correct URL called and response returned
  - Test: `http_tool_post_with_body` — mock server, create tool with POST + body template, execute, assert body sent correctly
  - Test: `http_tool_url_encoding` — param with spaces/special chars, assert URL-encoded in request
  - Test: `http_tool_response_jq` — mock JSON response, tool with jq path `.data.value`, assert extracted correctly
  - Test: `http_tool_timeout` — mock slow server (>10s), assert timeout error
  - Test: `http_tool_truncates_long_response` — mock response >2000 chars, assert truncated
  - Test: `extract_json_path_basic` — nested object, assert correct extraction
  - Test: `extract_json_path_array` — array index, assert works
  - Test: `extract_json_path_missing` — bad path, assert None
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.4 — Script tool executor

A generic tool that runs a Python or bash script as a subprocess.

- [ ] Create `src/tools/script_tool.rs`
- [ ] Define `ScriptTool` struct holding a `DynamicToolDef` (must be `Script` variant) and `data_dir: String`
- [ ] Implement `Tool` trait:
  - `name()` / `description()` / `parameters()` — from the definition
  - `tool_schema()` — build schema from parameters vec
  - `execute(params)`:
    1. Write the script source to a temp file in `{data_dir}/tmp/`
    2. Pass parameters as a JSON string via argv[1] (the script reads `sys.argv[1]` or `$1`)
    3. Run subprocess with `tokio::process::Command`, capture stdout + stderr
    4. 30-second timeout — kill the process if exceeded
    5. Return stdout on success, stderr on failure
    6. Clean up temp file
- [ ] Security: reject any script that contains `import os` + `system(`, `subprocess`, `shutil.rmtree`, `rm -rf`, `eval(`, or `exec(` with arguments from user input. This is a basic blocklist — not bulletproof, but catches obvious footguns. Log what was rejected.
- [ ] **TEST FIRST**:
  - Test: `script_tool_runs_python` — script that prints params, assert output matches
  - Test: `script_tool_runs_bash` — simple bash echo, assert output
  - Test: `script_tool_passes_params` — script that reads argv[1] JSON, assert params received correctly
  - Test: `script_tool_timeout` — script with `sleep(60)`, assert killed after timeout
  - Test: `script_tool_captures_stderr` — script that fails, assert stderr in error
  - Test: `script_tool_rejects_dangerous` — script with `subprocess.call`, assert rejected
  - Test: `script_tool_cleans_up_temp` — after execution, assert temp file removed
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.5 — Tool generator (Claude integration)

This is the brain of Phase 3. When Fern calls `request_tool`, this module asks Claude to design and generate a new tool.

- [ ] Create `src/tools/generator.rs`
- [ ] Define `TOOL_GENERATION_PROMPT` as a const:
  ```
  you are a tool designer for fern, a personal assistant chatbot. you create tools that fern can use to interact with the world.

  when given a description of what's needed, you design a tool and respond with ONLY a JSON object (no markdown, no explanation, no code fences).

  you can create two types of tools:

  TYPE 1 — HTTP tool (for calling APIs):
  {
    "name": "tool_name_snake_case",
    "description": "one-line description of what this tool does",
    "parameters": [
      { "name": "param_name", "param_type": "string", "description": "what this param is", "required": true }
    ],
    "tool_type": {
      "type": "Http",
      "url_template": "https://api.example.com/endpoint?q={{param_name}}",
      "method": "GET",
      "headers": { "Accept": "application/json" },
      "body_template": null,
      "response_jq": ".path.to.useful.data"
    }
  }

  TYPE 2 — Script tool (for local computation):
  {
    "name": "tool_name_snake_case",
    "description": "one-line description",
    "parameters": [
      { "name": "param_name", "param_type": "string", "description": "what this param is", "required": true }
    ],
    "tool_type": {
      "type": "Script",
      "interpreter": "python3",
      "source": "import sys, json\nparams = json.loads(sys.argv[1])\nprint(params['param_name'].upper())"
    }
  }

  rules:
  - prefer HTTP tools when an API exists for the task (weather, search, etc.)
  - use script tools for computation, text manipulation, or when no API exists
  - tool names must be snake_case, lowercase, no spaces
  - for HTTP tools, use free/no-auth APIs when possible (wttr.in, open-meteo, etc.)
  - if an API requires a key, include an "api_key" parameter so fern can ask the user for it
  - for script tools, the script MUST read params from sys.argv[1] as JSON
  - scripts should be self-contained — no pip installs, only stdlib
  - keep it minimal — one tool, one job
  - response_jq uses dot notation: .foo.bar[0].baz
  - url_template uses {{param_name}} for substitution (double curly braces)

  respond with ONLY the JSON. nothing else.
  ```
- [ ] Define `ToolGenerator` struct: `anthropic: Arc<AnthropicClient>`, `data_dir: String`
- [ ] Implement `pub async fn generate_tool(&self, request: &str) -> Result<DynamicToolDef, String>`:
  1. Call Claude with the generation prompt + the request description
  2. Parse response as JSON into `DynamicToolDef`
  3. Validate: name is snake_case, no empty fields, tool_type is valid
  4. If Script type, run through the security blocklist from 3.4
  5. Save to `{data_dir}/tools/{name}.json`
  6. Return the definition
- [ ] Implement `pub fn validate_tool_def(def: &DynamicToolDef) -> Result<(), String>` as a standalone function:
  - Name: only `[a-z0-9_]`, 3-50 chars
  - Description: non-empty, under 200 chars
  - At least one param OR explicitly no params
  - Http: url_template non-empty, method is valid HTTP method
  - Script: source non-empty, interpreter is "python3" or "bash"
- [ ] **TEST FIRST**:
  - Test: `validate_good_http_tool` — valid def, assert Ok
  - Test: `validate_good_script_tool` — valid def, assert Ok
  - Test: `validate_rejects_bad_name` — spaces in name, assert error
  - Test: `validate_rejects_empty_description` — assert error
  - Test: `validate_rejects_bad_method` — "YOLO" as method, assert error
  - Test: `validate_rejects_bad_interpreter` — "ruby" as interpreter, assert error
  - Test: `generate_tool_parses_claude_response` — mock Anthropic returning valid JSON, assert tool def created and saved
  - Test: `generate_tool_handles_bad_json` — mock Anthropic returning garbage, assert error
  - Test: `generate_tool_handles_markdown_wrapped` — mock response with ```json fences, assert still parses (strip fences before parsing)
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.6 — The `request_tool` tool

This is the meta-tool that Fern calls when it doesn't have what it needs. It triggers the generation pipeline from 3.5 and registers the result into the live registry.

- [ ] Create `src/tools/request_tool.rs`
- [ ] Define `RequestToolTool` struct (yes, the name is redundant — that's fine):
  - `generator: Arc<ToolGenerator>`
  - `registry: Arc<RwLock<ToolRegistry>>` — note: registry needs to become `RwLock` so we can add tools at runtime
  - `data_dir: String`
- [ ] Implement `Tool` trait:
  - name: `"request_tool"`
  - description: `"request a new tool when you can't do something with your current tools. describe in detail what you need — what it should do, what inputs it takes, what output you expect. a new tool will be generated and made available to you."`
  - params: `"description (string): detailed description of the tool you need, including what it does, expected inputs, and desired output format"`
  - `tool_schema()`: proper schema with description param
  - `execute(params)`:
    1. Extract `description` string from params
    2. Call `generator.generate_tool(description)`
    3. On success: build the appropriate executor (HttpTool or ScriptTool) from the def, register it in the live registry
    4. Return: `"new tool '{name}' created and ready to use: {description}. you can now call it."`
    5. On failure: return the error so Fern can tell the user what went wrong
- [ ] **Update `ToolRegistry`**: change `tools: HashMap<...>` to be behind `RwLock` so request_tool can insert at runtime. Alternatively, make `ToolRegistry` use interior mutability. The simplest approach: `RequestToolTool` holds an `Arc<Mutex<ToolRegistry>>` and the orchestrator uses the same Arc.
  - Update all existing references to `ToolRegistry` (orchestrator, etc.) to use `Arc<Mutex<ToolRegistry>>` or `Arc<RwLock<ToolRegistry>>`
  - `register()` takes `&self` instead of `&mut self` with interior mutability
- [ ] **TEST FIRST**:
  - Test: `request_tool_generates_and_registers` — mock Anthropic, call execute with a description, assert tool now exists in registry
  - Test: `request_tool_returns_success_message` — assert response mentions the new tool name
  - Test: `request_tool_handles_generation_failure` — mock Anthropic returning garbage, assert error message returned (not panic)
  - Test: `request_tool_missing_description` — empty params, assert error
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.7 — Boot-time tool loading

On startup, Fern should scan `{data_dir}/tools/` and register any saved dynamic tools so they survive restarts.

- [ ] Create `src/tools/loader.rs`
- [ ] Implement `pub fn load_and_register_tools(data_dir: &str, registry: &mut ToolRegistry)`:
  1. Call `load_all_tools(data_dir)` from dynamic.rs
  2. For each definition:
     a. Validate with `validate_tool_def`
     b. Build the appropriate executor (HttpTool or ScriptTool)
     c. Register in the registry
     d. Log: `"loaded dynamic tool: {name} ({type})"`
  3. Log total count at the end
- [ ] **TEST FIRST** (use tempdir):
  - Test: `loads_http_tool_on_boot` — save an HTTP tool def to disk, call load_and_register, assert tool exists in registry
  - Test: `loads_script_tool_on_boot` — same for script
  - Test: `loads_multiple_tools` — save 3, load, assert all 3 registered
  - Test: `skips_invalid_on_boot` — save a corrupt file, assert others still load, no panic
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.8 — Update orchestrator prompt

Fern needs to know about `request_tool` and when to use it. The orchestrator prompt gets an update.

- [ ] Update `ORCHESTRATOR_PROMPT` in `src/orchestrator/mod.rs`:
  ```
  you are fern's brain. you receive a message and decide what to do.

  you can:
  1. respond directly with text (for simple conversation)
  2. call tools when needed, then respond
  3. if you don't have a tool for something, call request_tool to create one

  about request_tool:
  - use it when someone asks you to do something and none of your current tools can help
  - describe exactly what you need: what the tool should do, what inputs it takes, what output you want
  - be specific — "i need a tool that fetches current weather for a city using a free API, takes a location string, returns temperature and conditions" is much better than "weather tool"
  - after request_tool succeeds, you'll have a new tool available — use it right away to answer the user's question
  - don't ask the user for permission to create tools — just do it. you can mention you made something new in your response though
  - if tool creation fails, let the user know and suggest alternatives

  rules:
  - use the provided tool schema directly
  - call only tools that exist in the schema (or request_tool to make new ones)
  - if you call a tool, wait for its result before deciding next step
  - if a tool fails, handle it gracefully and explain briefly
  - stay in character as fern (lowercase, casual, brief)
  ```
- [ ] Increase max tool calls per message from 5 to 8 in `engine.rs` — tool creation + usage in the same turn can chain: request_tool → new_tool → maybe memory_write = 3 calls minimum
- [ ] `cargo clippy` passes

## 3.9 — Wire everything in main.rs

- [ ] Update `Config` to include optional `anthropic_api_key` and `anthropic_model`
- [ ] Update `main.rs`:
  1. Create `AnthropicClient` if API key is present
  2. Create `ToolGenerator` with the Anthropic client
  3. Build `ToolRegistry` and register static tools (memory, time, reminders)
  4. Call `load_and_register_tools(data_dir, &mut registry)` to load persisted dynamic tools
  5. Register `RequestToolTool` if Anthropic client is available (graceful degradation: no API key = no tool creation, but everything else works)
  6. Wrap registry for shared access (Arc<RwLock> or Arc<Mutex>)
  7. Pass to orchestrator as before
- [ ] Handle the case where `ANTHROPIC_API_KEY` is not set: Fern works fine with existing tools, `request_tool` just isn't available
- [ ] `cargo build` succeeds
- [ ] `cargo test` — ALL tests pass
- [ ] `cargo clippy` + `cargo fmt` clean

## 3.10 — Tool management command

Users should be able to see and manage dynamic tools.

- [ ] Add `/tools` command handler in `bot.rs`:
  - `/tools` — list all registered tools (name + description + whether it's built-in or dynamic)
  - `/tools delete <name>` — delete a dynamic tool (reject deleting built-in tools)
  - Format the response as readable plain text, not markdown
- [ ] **TEST FIRST**:
  - Test: `tools_command_lists_all` — register some tools, simulate command, assert all listed
  - Test: `tools_delete_removes_dynamic` — save a dynamic tool, delete command, assert removed from registry and disk
  - Test: `tools_delete_rejects_builtin` — try deleting "memory_read", assert rejected
- [ ] Make all tests pass
- [ ] `cargo clippy` passes

## 3.11 — Deploy and manual test

- [ ] Push and rebuild:
  ```bash
  cd /opt/fern/app && git pull
  cd /opt/fern && docker compose build --no-cache fern
  docker volume rm fern_fern_data
  docker compose up -d fern
  sleep 5
  docker compose logs fern --since 1m
  ```
- [ ] Add `ANTHROPIC_API_KEY` to the `.env` file on the VPS
- [ ] Test basic conversation still works:
  1. "hey fern" → normal response
- [ ] Test tool creation — weather:
  2. "what's the weather in austin?" → Fern should call request_tool, Claude generates an HTTP tool (probably wttr.in or open-meteo), Fern uses it, responds with weather
  3. Watch logs: `docker compose logs fern --since 2m --follow`
  4. "what's the weather in tokyo?" → should reuse the same tool without creating a new one
- [ ] Test tool creation — computation:
  5. "convert 72°F to celsius for me" → might create a script tool, or might just answer directly (either is fine)
- [ ] Test tool persistence:
  6. `/tools` → should list the new weather tool alongside built-in ones
  7. Restart: `docker compose restart fern`
  8. "what's the weather in london?" → should use the persisted tool without recreating
- [ ] Test tool deletion:
  9. `/tools delete weather` (or whatever it's named)
  10. `/tools` → should no longer show it
- [ ] Test error handling:
  11. Ask something that would need an API key → Fern should explain the issue
- [ ] Check logs and tool storage:
  ```bash
  docker compose logs fern --since 5m | grep -i "tool\|generator\|anthropic\|request"
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim ls -la /data/tools/
  docker run --rm -v fern_fern_data:/data debian:bookworm-slim cat /data/tools/*.json
  ```

## 3.12 — Cleanup and commit

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — zero warnings
- [ ] `cargo fmt` — formatted
- [ ] No hardcoded secrets (Anthropic key comes from env only)
- [ ] `git add -A && git commit -m "phase 3: self-expanding tool system with claude generation"`
- [ ] Tag: `git tag v0.4.0-self-tools`

---

## Phase 3 completion criteria

All must be true:

1. `cargo test` passes with all tests green
2. `cargo clippy` and `cargo fmt` report zero issues
3. Anthropic client can call Claude and get responses
4. HTTP tools work — template rendering, request execution, response extraction
5. Script tools work — subprocess execution with timeout and security checks
6. `request_tool` generates new tools via Claude and registers them at runtime
7. Dynamic tools persist to disk and load on boot
8. Orchestrator prompt guides Fern to use `request_tool` when it lacks capability
9. `/tools` command lists tools, `/tools delete` removes dynamic ones
10. Fern can handle the full loop: user asks → Fern realizes it can't → creates tool → uses tool → answers
11. System degrades gracefully without `ANTHROPIC_API_KEY` (no tool creation, but everything else works)
12. Max 8 tool calls per message (no infinite loops)
13. Script security blocklist prevents obvious dangerous code

---

## Phase 4 preview (what could come next)

- **Tool improvement**: Fern notices a tool keeps failing and asks Claude to fix it
- **Tool composition**: Fern chains multiple tools together for complex workflows
- **User-provided API keys**: Fern asks users for API keys when needed, stores them in memory
- **MCP server**: Expose Fern's tool registry as an MCP server so other clients can use its tools
- **Approval flow**: For sensitive tool creation, Fern asks the user before generating
