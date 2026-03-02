use std::{path::Path, sync::Arc};

use futures_util::future::BoxFuture;
use sqlx::SqlitePool;

use crate::{
    ai::cerebras::{CerebrasClient, ChatMessage},
    db::messages::{get_recent_messages, save_message, upsert_user},
    engine::conversation::FERN_SYSTEM_PROMPT,
    orchestrator::{parse_response, OrchestratorAction, ORCHESTRATOR_PROMPT},
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
            .map(|stored| ChatMessage {
                role: stored.role,
                content: stored.content,
            })
            .collect::<Vec<_>>();

        let memory = read_memory_file(&self.data_dir);
        let system_prompt = format!(
            "{FERN_SYSTEM_PROMPT}\n\ncurrent memory:\n{memory}\n\n{ORCHESTRATOR_PROMPT}\n\n{}",
            self.registry.build_tools_prompt()
        );

        let mut tool_calls = 0usize;
        loop {
            let ai_response = self
                .cerebras
                .chat(&system_prompt, history.clone())
                .await
                .map_err(|err| format!("failed to run orchestrator model: {err}"))?;

            match parse_response(&ai_response) {
                OrchestratorAction::Respond(text) => {
                    save_message(&self.db, user_id, room_id, "assistant", &text)
                        .await
                        .map_err(|err| format!("failed to save assistant message: {err}"))?;
                    return Ok(text);
                }
                OrchestratorAction::CallTool {
                    tool_name,
                    params,
                    interim_text,
                } => {
                    if let Some(interim) = interim_text {
                        if let Err(err) = send_fn(interim).await {
                            tracing::warn!(error = %err, "failed to send interim orchestrator message");
                        }
                    }

                    if tool_calls >= 5 {
                        let text =
                            "hmm i'm getting stuck in tool calls right now, try again in a sec 🌿"
                                .to_owned();
                        save_message(&self.db, user_id, room_id, "assistant", &text)
                            .await
                            .map_err(|err| format!("failed to save assistant message: {err}"))?;
                        return Ok(text);
                    }
                    tool_calls += 1;

                    let Some(tool) = self.registry.get(&tool_name) else {
                        let text = format!("hmm i couldn't find a tool named {tool_name}");
                        save_message(&self.db, user_id, room_id, "assistant", &text)
                            .await
                            .map_err(|err| format!("failed to save assistant message: {err}"))?;
                        return Ok(text);
                    };

                    match tool.execute(params).await {
                        Ok(result) => history.push(ChatMessage {
                            role: "system".to_owned(),
                            content: format!("[tool:{tool_name} result] {result}"),
                        }),
                        Err(err) => {
                            let text =
                                format!("hmm the {tool_name} tool failed: {err}. try again?");
                            save_message(&self.db, user_id, room_id, "assistant", &text)
                                .await
                                .map_err(|save_err| {
                                    format!("failed to save assistant message: {save_err}")
                                })?;
                            return Ok(text);
                        }
                    }
                }
            }
        }
    }
}

fn read_memory_file(data_dir: &str) -> String {
    let path = Path::new(data_dir).join("memory.md");
    std::fs::read_to_string(path).unwrap_or_default()
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
        let orchestrator =
            Orchestrator::new(cerebras, Arc::clone(&registry), "./data".to_owned(), db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(SequenceResponder {
                calls: Arc::new(AtomicUsize::new(0)),
                first: json!({
                    "choices": [{
                        "message": { "content": "<tool_call>{\"tool\":\"dummy_tool\",\"params\":{}}</tool_call>" }
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
        assert!(second_body.contains("[tool:dummy_tool result] 42"));
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
                        "message": { "content": "one sec\n<tool_call>{\"tool\":\"dummy_tool\",\"params\":{}}</tool_call>" }
                    }]
                }),
                second: json!({
                    "choices": [{
                        "message": { "content": "done" }
                    }]
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
        assert_eq!(calls.as_slice(), ["one sec"]);
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
                    "message": { "content": "<tool_call>{\"tool\":\"dummy_tool\",\"params\":{}}</tool_call>" }
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
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "content": "<tool_call>{\"tool\":\"missing_tool\",\"params\":{}}</tool_call>" }
                }]
            })))
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
}
