use std::sync::Arc;

use sqlx::SqlitePool;

use crate::{
    ai::cerebras::{CerebrasClient, ChatMessage},
    db::messages::{get_recent_messages, save_message, upsert_user},
    memory::PERSONALITY_TEMPLATE,
};

pub type EngineResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct ConversationEngine {
    pub cerebras: Arc<CerebrasClient>,
    pub db: SqlitePool,
}

impl ConversationEngine {
    pub fn new(cerebras: Arc<CerebrasClient>, db: SqlitePool) -> Self {
        Self { cerebras, db }
    }

    pub async fn respond(
        &self,
        user_id: &str,
        room_id: &str,
        message: &str,
    ) -> EngineResult<String> {
        upsert_user(&self.db, user_id, None).await?;
        save_message(&self.db, user_id, room_id, "user", message).await?;

        let recent = get_recent_messages(&self.db, room_id, 30).await?;
        let history = recent
            .into_iter()
            .map(|stored| ChatMessage::new(stored.role, stored.content))
            .collect::<Vec<_>>();

        let response = match self
            .cerebras
            .chat(PERSONALITY_TEMPLATE, history, None)
            .await
        {
            Ok(response) => response
                .choices
                .into_iter()
                .next()
                .and_then(|choice| choice.message.content)
                .unwrap_or_else(|| "hmm i got an empty response, try again?".to_owned()),
            Err(err) => {
                tracing::error!(error = %err, "failed to generate ai response");
                "hmm something went wrong on my end, give me a sec 🌿".to_owned()
            }
        };

        save_message(&self.db, user_id, room_id, "assistant", &response).await?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wiremock::{
        matchers::{body_json, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::{
        ai::cerebras::CerebrasClient,
        db::{init_db, messages::save_message},
        Config,
    };

    use super::ConversationEngine;

    fn test_config(base_url: String) -> Config {
        Config {
            homeserver_url: "http://localhost:6167".to_owned(),
            bot_user: "@fern:example.org".to_owned(),
            bot_password: "password".to_owned(),
            data_dir: "./data".to_owned(),
            cerebras_api_key: "test-key".to_owned(),
            cerebras_model: "llama3.1-8b".to_owned(),
            cerebras_base_url: base_url,
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-20250514".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        }
    }

    #[tokio::test]
    async fn respond_saves_user_and_messages() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let engine = ConversationEngine::new(cerebras, db.clone());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "hey there" } }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let response = engine
            .respond("@alice:example.org", "!room:example.org", "hello")
            .await
            .expect("respond should succeed");

        assert_eq!(response, "hey there");

        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ?")
            .bind("@alice:example.org")
            .fetch_one(&db)
            .await
            .expect("query should succeed");
        assert_eq!(user_count, 1);

        let messages: Vec<(String, String)> = sqlx::query_as(
            "SELECT role, content
             FROM messages
             WHERE room_id = ?
             ORDER BY datetime(created_at), rowid;",
        )
        .bind("!room:example.org")
        .fetch_all(&db)
        .await
        .expect("query should succeed");

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], ("user".to_owned(), "hello".to_owned()));
        assert_eq!(
            messages[1],
            ("assistant".to_owned(), "hey there".to_owned())
        );
    }

    #[tokio::test]
    async fn respond_includes_history() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let engine = ConversationEngine::new(cerebras, db.clone());

        sqlx::query("INSERT INTO users (id, display_name) VALUES (?, ?)")
            .bind("@alice:example.org")
            .bind("Alice")
            .execute(&db)
            .await
            .expect("insert user should succeed");

        save_message(
            &db,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "earlier user",
        )
        .await
        .expect("save should succeed");
        save_message(
            &db,
            "@alice:example.org",
            "!room:example.org",
            "assistant",
            "earlier assistant",
        )
        .await
        .expect("save should succeed");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_json(json!({
                "model": "llama3.1-8b",
                "messages": [
                    { "role": "system", "content": crate::memory::PERSONALITY_TEMPLATE },
                    { "role": "user", "content": "earlier user" },
                    { "role": "assistant", "content": "earlier assistant" },
                    { "role": "user", "content": "latest input" }
                ],
                "max_tokens": 512,
                "temperature": 0.7
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "ok got it" } }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let _ = engine
            .respond("@alice:example.org", "!room:example.org", "latest input")
            .await
            .expect("respond should succeed");
    }

    #[tokio::test]
    async fn respond_handles_ai_failure() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let engine = ConversationEngine::new(cerebras, db);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("nope"))
            .mount(&server)
            .await;

        let response = engine
            .respond("@alice:example.org", "!room:example.org", "hello")
            .await
            .expect("respond should not return Err on ai failure");

        assert_eq!(
            response,
            "hmm something went wrong on my end, give me a sec 🌿"
        );
    }
}
