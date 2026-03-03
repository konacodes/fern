use std::{sync::Arc, time::Duration};

use chrono::{Local, TimeZone};
use sqlx::SqlitePool;

use crate::{
    ai::cerebras::{CerebrasClient, ChatMessage},
    db::messages::{get_messages_since, StoredMessage},
    memory::{read_behaviors, read_memory, write_memory},
};

pub const CONSOLIDATION_PROMPT: &str = r##"you maintain a memory file for fern, a personal assistant. you've been given today's chat log and the current memory file.

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
- you will also receive fern's current behaviors file. if any behavioral patterns seem stale, outdated, or contradicted by recent conversations, note them for removal. but do NOT rewrite behaviors.md — only update memory.md.

respond with ONLY the updated memory file contents. no explanation, no code fences, no preamble."##;

pub struct Consolidator {
    pub cerebras: Arc<CerebrasClient>,
    pub db: SqlitePool,
    pub data_dir: String,
}

impl Consolidator {
    pub fn new(cerebras: Arc<CerebrasClient>, db: SqlitePool, data_dir: String) -> Self {
        Self {
            cerebras,
            db,
            data_dir,
        }
    }

    pub async fn run_consolidation(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let memory = read_memory(&self.data_dir);
        let behaviors = read_behaviors(&self.data_dir);
        let now = Local::now();
        let midnight_naive = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .ok_or("failed to compute midnight")?;
        let since = Local
            .from_local_datetime(&midnight_naive)
            .earliest()
            .unwrap_or(now);

        let messages = get_messages_since(&self.db, since).await?;
        if messages.is_empty() {
            tracing::info!("no messages since midnight, skipping consolidation");
            return Ok(());
        }

        let chat_log = format_chat_log(&messages);
        let user_message = format!(
            "current memory:\n{memory}\n\ncurrent behaviors:\n{behaviors}\n\nchat log:\n{chat_log}"
        );
        let response = self
            .cerebras
            .chat(
                CONSOLIDATION_PROMPT,
                vec![ChatMessage::new("user", user_message)],
                None,
            )
            .await?;
        let response_text = response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .unwrap_or_default();

        if response_text.starts_with("# Fern's Memory") {
            write_memory(&self.data_dir, &response_text)?;
        } else {
            tracing::warn!("consolidation response invalid, keeping existing memory");
        }

        Ok(())
    }
}

pub fn format_chat_log(messages: &[StoredMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }

    messages
        .iter()
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn duration_until_next_midnight() -> Duration {
    let now = Local::now();
    let tomorrow = now
        .date_naive()
        .checked_add_signed(chrono::Duration::days(1))
        .unwrap_or(now.date_naive());
    let midnight_naive = match tomorrow.and_hms_opt(0, 0, 0) {
        Some(value) => value,
        None => return Duration::from_secs(1),
    };
    let next_midnight = match Local.from_local_datetime(&midnight_naive).earliest() {
        Some(value) => value,
        None => return Duration::from_secs(1),
    };

    next_midnight
        .signed_duration_since(now)
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(1))
}

pub async fn run_nightly_loop(consolidator: Arc<Consolidator>) {
    loop {
        let sleep_duration = duration_until_next_midnight();
        tokio::time::sleep(sleep_duration).await;
        if let Err(err) = consolidator.run_consolidation().await {
            tracing::error!(error = %err, "nightly consolidation failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration as ChronoDuration, Local};
    use serde_json::json;
    use tempfile::tempdir;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::{
        ai::cerebras::CerebrasClient,
        config::Config,
        db::{
            init_db,
            messages::{get_messages_since, save_message, upsert_user},
        },
        memory::{read_behaviors, read_memory, write_behaviors, write_memory},
    };

    use super::{duration_until_next_midnight, format_chat_log, Consolidator};

    fn test_config(base_url: String) -> Config {
        Config {
            homeserver_url: "http://localhost:6167".to_owned(),
            bot_user: "@fern:example.org".to_owned(),
            bot_password: "password".to_owned(),
            data_dir: "./data".to_owned(),
            cerebras_api_key: "test-key".to_owned(),
            cerebras_model: "qwen-3-235b".to_owned(),
            cerebras_base_url: base_url,
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-20250514".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        }
    }

    #[test]
    fn format_chat_log_basic() {
        let messages = vec![
            crate::db::messages::StoredMessage {
                id: "1".to_owned(),
                user_id: "@alice:example.org".to_owned(),
                room_id: "!room:example.org".to_owned(),
                role: "user".to_owned(),
                content: "hello".to_owned(),
                created_at: "2026-03-02 10:00:00".to_owned(),
            },
            crate::db::messages::StoredMessage {
                id: "2".to_owned(),
                user_id: "@fern:example.org".to_owned(),
                room_id: "!room:example.org".to_owned(),
                role: "assistant".to_owned(),
                content: "hey there".to_owned(),
                created_at: "2026-03-02 10:00:01".to_owned(),
            },
            crate::db::messages::StoredMessage {
                id: "3".to_owned(),
                user_id: "@alice:example.org".to_owned(),
                room_id: "!room:example.org".to_owned(),
                role: "user".to_owned(),
                content: "remember this".to_owned(),
                created_at: "2026-03-02 10:00:02".to_owned(),
            },
        ];

        let transcript = format_chat_log(&messages);
        assert!(transcript.contains("user: hello"));
        assert!(transcript.contains("assistant: hey there"));
        assert!(transcript.contains("user: remember this"));
    }

    #[test]
    fn format_chat_log_empty() {
        let transcript = format_chat_log(&[]);
        assert!(transcript.is_empty());
    }

    #[tokio::test]
    async fn consolidation_updates_memory() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        upsert_user(&db, "@alice:example.org", Some("Alice"))
            .await
            .expect("upsert should succeed");
        save_message(
            &db,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "hey",
        )
        .await
        .expect("save should succeed");

        let initial = "# Fern's Memory\n\n## Working Memory\n- (empty)\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)";
        write_memory(&data_dir, initial).expect("write should succeed");

        let updated = "# Fern's Memory\n\n## Working Memory\n- follow up about calendar\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)";

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": updated } }]
            })))
            .mount(&server)
            .await;

        let consolidator = Consolidator::new(cerebras, db, data_dir.clone());
        consolidator
            .run_consolidation()
            .await
            .expect("consolidation should succeed");

        let memory = read_memory(&data_dir);
        assert_eq!(memory, updated);
    }

    #[tokio::test]
    async fn consolidation_skips_empty_day() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let initial = "# Fern's Memory\n\n## Working Memory\n- unchanged\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)";
        write_memory(&data_dir, initial).expect("write should succeed");

        let consolidator = Consolidator::new(cerebras, db, data_dir.clone());
        consolidator
            .run_consolidation()
            .await
            .expect("consolidation should succeed");

        let memory = read_memory(&data_dir);
        assert_eq!(memory, initial);
    }

    #[tokio::test]
    async fn consolidation_rejects_bad_response() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        upsert_user(&db, "@alice:example.org", Some("Alice"))
            .await
            .expect("upsert should succeed");
        save_message(
            &db,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "hey",
        )
        .await
        .expect("save should succeed");

        let initial = "# Fern's Memory\n\n## Working Memory\n- unchanged\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)";
        write_memory(&data_dir, initial).expect("write should succeed");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "not memory format" } }]
            })))
            .mount(&server)
            .await;

        let consolidator = Consolidator::new(cerebras, db, data_dir.clone());
        consolidator
            .run_consolidation()
            .await
            .expect("consolidation should succeed");

        let memory = read_memory(&data_dir);
        assert_eq!(memory, initial);
    }

    #[tokio::test]
    async fn consolidation_receives_behaviors() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        upsert_user(&db, "@alice:example.org", Some("Alice"))
            .await
            .expect("upsert should succeed");
        save_message(
            &db,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "hey",
        )
        .await
        .expect("save should succeed");
        write_memory(
            &data_dir,
            "# Fern's Memory\n\n## Working Memory\n- (empty)\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)",
        )
        .expect("memory write should succeed");
        let behaviors = "# Fern's Learned Behaviors\n\n## General\n- include sources for news";
        write_behaviors(&data_dir, behaviors).expect("behaviors write should succeed");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "# Fern's Memory\n\n## Working Memory\n- updated\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)" } }]
            })))
            .mount(&server)
            .await;

        let consolidator = Consolidator::new(cerebras, db, data_dir);
        consolidator
            .run_consolidation()
            .await
            .expect("consolidation should succeed");

        let requests = server
            .received_requests()
            .await
            .expect("requests should be available");
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body should parse");
        let prompt = body
            .get("messages")
            .and_then(serde_json::Value::as_array)
            .and_then(|messages| messages.get(1))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str)
            .expect("user content should be present");
        assert!(prompt.contains("include sources for news"));
    }

    #[tokio::test]
    async fn consolidation_still_only_writes_memory() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        upsert_user(&db, "@alice:example.org", Some("Alice"))
            .await
            .expect("upsert should succeed");
        save_message(
            &db,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "hey",
        )
        .await
        .expect("save should succeed");
        let initial_memory = "# Fern's Memory\n\n## Working Memory\n- (empty)\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)";
        write_memory(&data_dir, initial_memory).expect("memory write should succeed");
        let initial_behaviors =
            "# Fern's Learned Behaviors\n\n## Tool Usage\n- use search_tools first";
        write_behaviors(&data_dir, initial_behaviors).expect("behaviors write should succeed");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "# Fern's Memory\n\n## Working Memory\n- updated\n\n## Projects & Work\n- (empty)\n\n## Preferences & Style\n- (empty)\n\n## Long-Term Memory\n- (empty)" } }]
            })))
            .mount(&server)
            .await;

        let consolidator = Consolidator::new(cerebras, db, data_dir.clone());
        consolidator
            .run_consolidation()
            .await
            .expect("consolidation should succeed");

        let saved_behaviors = read_behaviors(&data_dir);
        assert_eq!(saved_behaviors, initial_behaviors);
    }

    #[tokio::test]
    async fn get_messages_since_filters() {
        let db = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");
        upsert_user(&db, "@alice:example.org", Some("Alice"))
            .await
            .expect("upsert should succeed");

        sqlx::query(
            "INSERT INTO messages (id, user_id, room_id, role, content, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("old-msg")
        .bind("@alice:example.org")
        .bind("!room:example.org")
        .bind("user")
        .bind("old")
        .bind(
            (Local::now() - ChronoDuration::days(1))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        )
        .execute(&db)
        .await
        .expect("insert should succeed");

        save_message(
            &db,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "new",
        )
        .await
        .expect("save should succeed");

        let since = Local::now() - ChronoDuration::hours(1);
        let messages = get_messages_since(&db, since)
            .await
            .expect("query should succeed");

        assert!(messages.iter().any(|m| m.content == "new"));
        assert!(messages.iter().all(|m| m.content != "old"));
    }

    #[test]
    fn duration_until_midnight_positive() {
        let duration = duration_until_next_midnight();
        assert!(duration > std::time::Duration::from_secs(0));
        assert!(duration <= std::time::Duration::from_secs(24 * 60 * 60));
    }
}
