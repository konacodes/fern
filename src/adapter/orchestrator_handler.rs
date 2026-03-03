use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::BoxFuture;
use sqlx::SqlitePool;

use crate::{
    adapter::{MessageHandler, MessagingAdapter},
    db::messages::delete_room_messages,
    memory::{write_memory, MEMORY_TEMPLATE},
    orchestrator::engine::Orchestrator,
};

pub struct FernHandler {
    orchestrator: Arc<Orchestrator>,
    adapter: Arc<dyn MessagingAdapter>,
    data_dir: String,
    db: SqlitePool,
}

impl FernHandler {
    pub fn new(
        orchestrator: Arc<Orchestrator>,
        adapter: Arc<dyn MessagingAdapter>,
        data_dir: String,
        db: SqlitePool,
    ) -> Self {
        Self {
            orchestrator,
            adapter,
            data_dir,
            db,
        }
    }
}

#[async_trait]
impl MessageHandler for FernHandler {
    async fn handle_message(
        &self,
        sender_id: &str,
        conversation_id: &str,
        text: &str,
    ) -> Result<String, String> {
        if text.trim() == "/reset" {
            write_memory(&self.data_dir, MEMORY_TEMPLATE)
                .map_err(|err| format!("failed to reset memory: {err}"))?;
            delete_room_messages(&self.db, conversation_id)
                .await
                .map_err(|err| format!("failed to clear conversation messages: {err}"))?;
            return Ok("factory reset complete 🌿 fresh start".to_owned());
        }

        if text.trim() == "/tools" {
            let guard = self
                .orchestrator
                .registry
                .read()
                .map_err(|_| "failed to acquire tool registry read lock".to_owned())?;
            let tools = guard
                .list()
                .into_iter()
                .map(|(name, description, params)| {
                    (name.to_owned(), description.to_owned(), params.to_owned())
                })
                .collect::<Vec<_>>();
            if tools.is_empty() {
                return Ok("no tools registered".to_owned());
            }
            let mut response = "available tools:".to_owned();
            for (name, description, _) in &tools {
                response.push_str(&format!("\n- {name}: {description}"));
            }
            return Ok(response);
        }

        let adapter = Arc::clone(&self.adapter);
        let conversation_id_owned = conversation_id.to_owned();
        let send_fn = move |message: String| -> BoxFuture<'static, Result<(), String>> {
            let adapter = Arc::clone(&adapter);
            let conversation_id_owned = conversation_id_owned.clone();
            Box::pin(async move { adapter.send_message(&conversation_id_owned, &message).await })
        };

        self.orchestrator
            .process_message(sender_id, conversation_id, text, send_fn)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, RwLock};

    use async_trait::async_trait;
    use serde_json::json;
    use sqlx::SqlitePool;
    use tempfile::tempdir;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::{
        adapter::{MessageHandler, MessagingAdapter},
        ai::cerebras::CerebrasClient,
        config::Config,
        db::{
            init_db,
            messages::{save_message, upsert_user},
        },
        memory::{read_memory, write_memory, MEMORY_TEMPLATE},
        orchestrator::engine::Orchestrator,
        tools::ToolRegistry,
    };

    use super::FernHandler;

    struct MockAdapter {
        sent: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait]
    impl MessagingAdapter for MockAdapter {
        async fn run(&self, _handler: Arc<dyn MessageHandler>) -> Result<(), String> {
            Ok(())
        }

        async fn send_message(&self, conversation_id: &str, text: &str) -> Result<(), String> {
            self.sent
                .lock()
                .map_err(|_| "lock poisoned".to_owned())?
                .push((conversation_id.to_owned(), text.to_owned()));
            Ok(())
        }
    }

    fn test_config(base_url: String) -> Config {
        Config {
            signal_api_url: "http://signal-api:8080".to_owned(),
            signal_account_number: "+15550000000".to_owned(),
            data_dir: "./data".to_owned(),
            cerebras_api_key: "test-key".to_owned(),
            cerebras_model: "qwen-3-235b".to_owned(),
            cerebras_base_url: base_url,
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-20250514".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        }
    }

    fn make_handler(
        db: SqlitePool,
        data_dir: String,
        cerebras: Arc<CerebrasClient>,
        registry: Arc<RwLock<ToolRegistry>>,
        adapter: Arc<dyn MessagingAdapter>,
    ) -> FernHandler {
        let orchestrator = Arc::new(Orchestrator::new(
            cerebras,
            registry,
            data_dir.clone(),
            db.clone(),
        ));
        FernHandler::new(orchestrator, adapter, data_dir, db)
    }

    #[tokio::test]
    async fn handler_reset_command() {
        let server = MockServer::start().await;
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        write_memory(
            &data_dir,
            "# Fern's Memory\n\n## Working Memory\n- old\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)",
        )
        .expect("memory write should succeed");
        upsert_user(&db, "user", None)
            .await
            .expect("user upsert should succeed");
        save_message(&db, "user", "conv", "user", "hello")
            .await
            .expect("message save should succeed");

        let adapter_sent = Arc::new(Mutex::new(Vec::new()));
        let adapter: Arc<dyn MessagingAdapter> = Arc::new(MockAdapter { sent: adapter_sent });
        let handler = make_handler(
            db.clone(),
            data_dir.clone(),
            Arc::new(CerebrasClient::new(&test_config(format!(
                "{}/v1",
                server.uri()
            )))),
            Arc::new(RwLock::new(ToolRegistry::new())),
            adapter,
        );

        let response = handler
            .handle_message("user", "conv", "/reset")
            .await
            .expect("reset should succeed");
        assert!(response.to_lowercase().contains("reset"));
        assert_eq!(read_memory(&data_dir), MEMORY_TEMPLATE);

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE room_id = ?")
            .bind("conv")
            .fetch_one(&db)
            .await
            .expect("query should succeed");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn handler_tools_command() {
        let server = MockServer::start().await;
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        {
            let mut guard = registry
                .write()
                .expect("registry lock should not be poisoned");
            guard.register_builtin(Box::new(crate::tools::time::CurrentTimeTool));
        }

        let adapter_sent = Arc::new(Mutex::new(Vec::new()));
        let adapter: Arc<dyn MessagingAdapter> = Arc::new(MockAdapter { sent: adapter_sent });
        let handler = make_handler(
            db,
            data_dir,
            Arc::new(CerebrasClient::new(&test_config(format!(
                "{}/v1",
                server.uri()
            )))),
            registry,
            adapter,
        );

        let response = handler
            .handle_message("user", "conv", "/tools")
            .await
            .expect("tools should succeed");
        assert!(response.contains("current_time"));
    }

    #[tokio::test]
    async fn handler_normal_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "ok direct" } }]
            })))
            .mount(&server)
            .await;

        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let adapter_sent = Arc::new(Mutex::new(Vec::new()));
        let adapter: Arc<dyn MessagingAdapter> = Arc::new(MockAdapter { sent: adapter_sent });
        let handler = make_handler(
            db,
            data_dir,
            Arc::new(CerebrasClient::new(&test_config(format!(
                "{}/v1",
                server.uri()
            )))),
            Arc::new(RwLock::new(ToolRegistry::new())),
            adapter,
        );

        let response = handler
            .handle_message("user", "conv", "hello")
            .await
            .expect("normal message should succeed");
        assert_eq!(response, "ok direct");

        let requests = server
            .received_requests()
            .await
            .expect("requests should be available");
        assert!(!requests.is_empty(), "orchestrator should call model");
    }
}
