use std::{path::Path, sync::Arc};

use futures_util::future::BoxFuture;
use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    ai::cerebras::{CerebrasClient, ChatMessage},
    db::messages::{get_recent_messages, save_message, upsert_user},
    engine::conversation::FERN_SYSTEM_PROMPT,
    orchestrator::ORCHESTRATOR_PROMPT,
    tools::ToolRegistry,
};

pub struct Orchestrator {
    pub cerebras: Arc<CerebrasClient>,
    pub registry: Arc<ToolRegistry>,
    pub data_dir: String,
    pub db: SqlitePool,
}

impl Orchestrator {
    pub fn new(
        cerebras: Arc<CerebrasClient>,
        registry: Arc<ToolRegistry>,
        data_dir: String,
        db: SqlitePool,
    ) -> Self {
        Self {
            cerebras,
            registry,
            data_dir,
            db,
        }
    }

    pub async fn process_message(
        &self,
        user_id: &str,
        room_id: &str,
        message: &str,
        send_fn: impl Fn(String) -> BoxFuture<'static, Result<(), String>>,
    ) -> Result<String, String> {
        tracing::info!(
            user_id,
            room_id,
            message = %truncate_for_log(message, 300),
            "orchestrator received user message"
        );
        upsert_user(&self.db, user_id, None)
            .await
            .map_err(|err| format!("failed to upsert user: {err}"))?;
        save_message(&self.db, user_id, room_id, "user", message)
            .await
            .map_err(|err| format!("failed to save user message: {err}"))?;

        let recent = get_recent_messages(&self.db, room_id, 30)
            .await
            .map_err(|err| format!("failed to load recent messages: {err}"))?;
        let mut history = recent
            .into_iter()
            .map(|stored| ChatMessage::new(stored.role, stored.content))
            .collect::<Vec<_>>();

        let memory = read_memory_file(&self.data_dir);
        let tools_schema = self.registry.build_tools_schema();
        let tool_names = extract_tool_names(&tools_schema);
        tracing::debug!(
            room_id,
            history_len = history.len(),
            memory_chars = memory.len(),
            tools_count = tools_schema.len(),
            tools = %tool_names.join(","),
            "orchestrator context prepared"
        );
        let system_prompt =
            format!("{FERN_SYSTEM_PROMPT}\n\ncurrent memory:\n{memory}\n\n{ORCHESTRATOR_PROMPT}");

        let mut total_tool_calls = 0usize;
        let mut loop_iteration = 0usize;
        loop {
            loop_iteration += 1;
            tracing::debug!(
                room_id,
                loop_iteration,
                total_tool_calls,
                history_len = history.len(),
                "starting orchestrator model turn"
            );
            let response = self
                .cerebras
                .chat(&system_prompt, history.clone(), Some(tools_schema.clone()))
                .await
                .map_err(|err| format!("failed to run orchestrator model: {err}"))?;
            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| "model returned no choices".to_owned())?;
            let message = choice.message;
            let tool_calls = message.tool_calls.clone().unwrap_or_default();
            tracing::debug!(
                room_id,
                loop_iteration,
                tool_calls = tool_calls.len(),
                assistant_content = %truncate_for_log(message.content.as_deref().unwrap_or(""), 500),
                "orchestrator model turn received"
            );

            if tool_calls.is_empty() {
                let text = message
                    .content
                    .unwrap_or_else(|| "hmm i got an empty response, try again?".to_owned());
                if looks_like_json(&text) {
                    tracing::warn!(
                        room_id,
                        loop_iteration,
                        text = %truncate_for_log(&text, 600),
                        "model returned json-like text with no tool_calls"
                    );
                }
                save_message(&self.db, user_id, room_id, "assistant", &text)
                    .await
                    .map_err(|err| format!("failed to save assistant message: {err}"))?;
                tracing::info!(
                    room_id,
                    loop_iteration,
                    response = %truncate_for_log(&text, 300),
                    "orchestrator returning final response"
                );
                return Ok(text);
            }

            if let Some(interim) = message.content.as_deref().map(str::trim) {
                if should_send_interim(interim) {
                    tracing::debug!(
                        room_id,
                        loop_iteration,
                        interim = %truncate_for_log(interim, 300),
                        "sending interim orchestrator message"
                    );
                    if let Err(err) = send_fn(interim.to_owned()).await {
                        tracing::warn!(error = %err, "failed to send interim orchestrator message");
                    }
                } else {
                    tracing::debug!(
                        room_id,
                        loop_iteration,
                        interim = %truncate_for_log(interim, 300),
                        "skipping interim message because it appears to be structured payload"
                    );
                }
            }

            history.push(ChatMessage {
                role: "assistant".to_owned(),
                content: message.content,
                tool_call_id: None,
                tool_calls: Some(tool_calls.clone()),
            });

            for tool_call in tool_calls {
                if total_tool_calls >= 5 {
                    let text =
                        "hmm i'm getting stuck in tool calls right now, try again in a sec 🌿"
                            .to_owned();
                    save_message(&self.db, user_id, room_id, "assistant", &text)
                        .await
                        .map_err(|err| format!("failed to save assistant message: {err}"))?;
                    return Ok(text);
                }
                total_tool_calls += 1;
                tracing::info!(
                    room_id,
                    loop_iteration,
                    total_tool_calls,
                    tool_call_id = %tool_call.id,
                    tool_name = %tool_call.function.name,
                    raw_arguments = %truncate_for_log(&tool_call.function.arguments, 800),
                    "executing tool call"
                );

                let mut params = match serde_json::from_str::<serde_json::Value>(
                    &tool_call.function.arguments,
                ) {
                    Ok(params) => params,
                    Err(err) => {
                        tracing::warn!(
                            room_id,
                            loop_iteration,
                            error = %err,
                            raw_arguments = %truncate_for_log(&tool_call.function.arguments, 800),
                            "failed to parse tool arguments as json; defaulting to empty object"
                        );
                        json!({})
                    }
                };

                if tool_call.function.name == "set_reminder" {
                    if let Some(map) = params.as_object_mut() {
                        map.entry("user_id".to_owned())
                            .or_insert_with(|| json!(user_id));
                        map.entry("room_id".to_owned())
                            .or_insert_with(|| json!(room_id));
                        tracing::debug!(
                            room_id,
                            loop_iteration,
                            tool_call_id = %tool_call.id,
                            injected_user_id = user_id,
                            injected_room_id = room_id,
                            "injected reminder context into tool params"
                        );
                    }
                }

                let tool_result = if let Some(tool) = self.registry.get(&tool_call.function.name) {
                    tracing::debug!(
                        room_id,
                        loop_iteration,
                        tool_call_id = %tool_call.id,
                        tool_name = %tool_call.function.name,
                        params = %truncate_for_log(&params.to_string(), 1000),
                        "calling tool implementation"
                    );
                    match tool.execute(params).await {
                        Ok(result) => result,
                        Err(err) => format!("error: {err}"),
                    }
                } else {
                    tracing::warn!(
                        room_id,
                        loop_iteration,
                        tool_name = %tool_call.function.name,
                        "tool not found in registry"
                    );
                    format!("error: unknown tool {}", tool_call.function.name)
                };
                tracing::info!(
                    room_id,
                    loop_iteration,
                    tool_call_id = %tool_call.id,
                    tool_result = %truncate_for_log(&tool_result, 300),
                    "tool execution finished"
                );

                history.push(ChatMessage::tool(tool_call.id, tool_result));
            }
        }
    }
}

fn read_memory_file(data_dir: &str) -> String {
    let path = Path::new(data_dir).join("memory.md");
    std::fs::read_to_string(path).unwrap_or_default()
}

fn should_send_interim(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Some models emit raw JSON alongside tool_calls. Don't leak that to users.
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return false;
    }

    true
}

fn looks_like_json(content: &str) -> bool {
    let trimmed = content.trim();
    (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
}

fn extract_tool_names(schema: &[serde_json::Value]) -> Vec<String> {
    schema
        .iter()
        .filter_map(|tool| {
            tool.get("function")
                .and_then(|func| func.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let truncated = value.chars().take(max_chars).collect::<String>();
    format!("{truncated}...[truncated]")
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use serde_json::json;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, Request, Respond, ResponseTemplate,
    };

    use crate::{
        ai::cerebras::CerebrasClient,
        config::Config,
        db::init_db,
        tools::{Tool, ToolRegistry},
    };

    use super::Orchestrator;

    struct DummyTool;

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy_tool"
        }

        fn description(&self) -> &str {
            "returns 42"
        }

        fn parameters(&self) -> &str {
            "none"
        }

        async fn execute(&self, _params: serde_json::Value) -> Result<String, String> {
            Ok("42".to_owned())
        }
    }

    struct SequenceResponder {
        calls: Arc<AtomicUsize>,
        first: serde_json::Value,
        second: serde_json::Value,
    }

    impl Respond for SequenceResponder {
        fn respond(&self, _request: &Request) -> ResponseTemplate {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                ResponseTemplate::new(200).set_body_json(self.first.clone())
            } else {
                ResponseTemplate::new(200).set_body_json(self.second.clone())
            }
        }
    }

    fn test_config(base_url: String) -> Config {
        Config {
            homeserver_url: "http://localhost:6167".to_owned(),
            bot_user: "@fern:example.org".to_owned(),
            bot_password: "password".to_owned(),
            data_dir: "./data".to_owned(),
            cerebras_api_key: "test-key".to_owned(),
            cerebras_model: "qwen-3-235b".to_owned(),
            cerebras_base_url: base_url,
            database_url: "sqlite::memory:".to_owned(),
        }
    }

    #[tokio::test]
    async fn process_direct_response() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let registry = Arc::new(ToolRegistry::new());
        let orchestrator = Orchestrator::new(cerebras, registry, "./data".to_owned(), db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "plain reply" } }]
            })))
            .mount(&server)
            .await;

        let response = orchestrator
            .process_message("user", "room", "hello", |_| Box::pin(async { Ok(()) }))
            .await
            .expect("process should succeed");

        assert_eq!(response, "plain reply");
    }

    #[tokio::test]
    async fn process_with_tool_call() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        let registry = Arc::new(registry);
        let orchestrator = Orchestrator::new(cerebras, registry, "./data".to_owned(), db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(SequenceResponder {
                calls: Arc::new(AtomicUsize::new(0)),
                first: json!({
                    "choices": [{
                        "message": {
                            "content": "one sec",
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": { "name": "dummy_tool", "arguments": "{}" }
                            }]
                        }
                    }]
                }),
                second: json!({
                    "choices": [{
                        "message": { "content": "final with tool context" }
                    }]
                }),
            })
            .mount(&server)
            .await;

        let response = orchestrator
            .process_message("user", "room", "what's the answer?", |_| {
                Box::pin(async { Ok(()) })
            })
            .await
            .expect("process should succeed");

        let requests = server
            .received_requests()
            .await
            .expect("requests should be available");
        assert!(requests.len() >= 2);
        let second_body =
            String::from_utf8(requests[1].body.clone()).expect("request body should be utf8");
        assert!(second_body.contains("\"role\":\"tool\""));
        assert!(second_body.contains("\"tool_call_id\":\"call_1\""));
        assert!(second_body.contains("\"content\":\"42\""));
        assert_eq!(response, "final with tool context");
    }

    #[tokio::test]
    async fn process_sends_interim_message() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        let registry = Arc::new(registry);
        let orchestrator = Orchestrator::new(cerebras, registry, "./data".to_owned(), db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(SequenceResponder {
                calls: Arc::new(AtomicUsize::new(0)),
                first: json!({
                    "choices": [{
                        "message": {
                            "content": "let me check",
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": { "name": "dummy_tool", "arguments": "{}" }
                            }]
                        }
                    }]
                }),
                second: json!({
                    "choices": [{ "message": { "content": "done" } }]
                }),
            })
            .mount(&server)
            .await;

        let sent = Arc::new(Mutex::new(Vec::<String>::new()));
        let sent_for_callback = Arc::clone(&sent);

        let _ = orchestrator
            .process_message("user", "room", "question", move |message| {
                let sent_for_callback = Arc::clone(&sent_for_callback);
                Box::pin(async move {
                    sent_for_callback
                        .lock()
                        .expect("send lock should not be poisoned")
                        .push(message);
                    Ok(())
                })
            })
            .await
            .expect("process should succeed");

        let calls = sent.lock().expect("send lock should not be poisoned");
        assert_eq!(calls.as_slice(), ["let me check"]);
    }

    #[tokio::test]
    async fn process_skips_json_interim_message() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        let registry = Arc::new(registry);
        let orchestrator = Orchestrator::new(cerebras, registry, "./data".to_owned(), db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(SequenceResponder {
                calls: Arc::new(AtomicUsize::new(0)),
                first: json!({
                    "choices": [{
                        "message": {
                            "content": "{\"type\":\"set_reminder\",\"delay_minutes\":1,\"message\":\"eat lunch\"}",
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": { "name": "dummy_tool", "arguments": "{}" }
                            }]
                        }
                    }]
                }),
                second: json!({
                    "choices": [{ "message": { "content": "done" } }]
                }),
            })
            .mount(&server)
            .await;

        let sent = Arc::new(Mutex::new(Vec::<String>::new()));
        let sent_for_callback = Arc::clone(&sent);

        let _ = orchestrator
            .process_message("user", "room", "question", move |message| {
                let sent_for_callback = Arc::clone(&sent_for_callback);
                Box::pin(async move {
                    sent_for_callback
                        .lock()
                        .expect("send lock should not be poisoned")
                        .push(message);
                    Ok(())
                })
            })
            .await
            .expect("process should succeed");

        let calls = sent.lock().expect("send lock should not be poisoned");
        assert!(
            calls.is_empty(),
            "json-formatted interim content should be filtered out"
        );
    }

    #[tokio::test]
    async fn process_max_tool_calls() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        let registry = Arc::new(registry);
        let orchestrator = Orchestrator::new(cerebras, registry, "./data".to_owned(), db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": {
                        "tool_calls": [{
                            "id": "call_loop",
                            "type": "function",
                            "function": { "name": "dummy_tool", "arguments": "{}" }
                        }]
                    }
                }]
            })))
            .mount(&server)
            .await;

        let response = orchestrator
            .process_message("user", "room", "loop forever", |_| {
                Box::pin(async { Ok(()) })
            })
            .await
            .expect("process should return graceful message");

        assert!(response.contains("stuck"));
    }

    #[tokio::test]
    async fn process_unknown_tool() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let registry = Arc::new(ToolRegistry::new());
        let orchestrator = Orchestrator::new(cerebras, registry, "./data".to_owned(), db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(SequenceResponder {
                calls: Arc::new(AtomicUsize::new(0)),
                first: json!({
                    "choices": [{
                        "message": {
                            "tool_calls": [{
                                "id": "call_missing",
                                "type": "function",
                                "function": { "name": "missing_tool", "arguments": "{}" }
                            }]
                        }
                    }]
                }),
                second: json!({
                    "choices": [{
                        "message": { "content": "i couldn't run missing_tool" }
                    }]
                }),
            })
            .mount(&server)
            .await;

        let response = orchestrator
            .process_message("user", "room", "try missing", |_| {
                Box::pin(async { Ok(()) })
            })
            .await
            .expect("process should return graceful message");

        assert!(response.contains("missing_tool"));
    }

    #[test]
    fn should_send_interim_filters_json() {
        assert!(!super::should_send_interim(
            "{\"type\":\"set_reminder\",\"delay_minutes\":1,\"message\":\"eat lunch\"}"
        ));
        assert!(!super::should_send_interim("  [1,2,3]  "));
        assert!(!super::should_send_interim("  "));
        assert!(super::should_send_interim("let me check that"));
    }
}
