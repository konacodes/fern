use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredMessage {
    pub id: String,
    pub user_id: String,
    pub room_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub async fn upsert_user(
    pool: &SqlitePool,
    user_id: &str,
    display_name: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, display_name)
         VALUES (?, ?)
         ON CONFLICT(id) DO UPDATE SET
             display_name = excluded.display_name,
             updated_at = datetime('now');",
    )
    .bind(user_id)
    .bind(display_name)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn save_message(
    pool: &SqlitePool,
    user_id: &str,
    room_id: &str,
    role: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO messages (id, user_id, room_id, role, content)
         VALUES (?, ?, ?, ?, ?);",
    )
    .bind(id)
    .bind(user_id)
    .bind(room_id)
    .bind(role)
    .bind(content)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_recent_messages(
    pool: &SqlitePool,
    room_id: &str,
    limit: i64,
) -> Result<Vec<StoredMessage>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, user_id, room_id, role, content, created_at
         FROM messages
         WHERE room_id = ?
         ORDER BY datetime(created_at) DESC, rowid DESC
         LIMIT ?;",
    )
    .bind(room_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut messages = rows
        .into_iter()
        .map(|row| StoredMessage {
            id: row.get::<String, _>("id"),
            user_id: row.get::<String, _>("user_id"),
            room_id: row.get::<String, _>("room_id"),
            role: row.get::<String, _>("role"),
            content: row.get::<String, _>("content"),
            created_at: row.get::<String, _>("created_at"),
        })
        .collect::<Vec<_>>();

    messages.reverse();
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use crate::db::init_db;

    use super::{get_recent_messages, save_message, upsert_user};

    #[tokio::test]
    async fn save_and_retrieve_messages() {
        let pool = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");

        upsert_user(&pool, "@alice:example.org", Some("Alice"))
            .await
            .expect("upsert should succeed");
        save_message(
            &pool,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "first",
        )
        .await
        .expect("save should succeed");
        save_message(
            &pool,
            "@alice:example.org",
            "!room:example.org",
            "assistant",
            "second",
        )
        .await
        .expect("save should succeed");
        save_message(
            &pool,
            "@alice:example.org",
            "!room:example.org",
            "user",
            "third",
        )
        .await
        .expect("save should succeed");

        let messages = get_recent_messages(&pool, "!room:example.org", 10)
            .await
            .expect("read should succeed");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "first");
        assert_eq!(messages[1].content, "second");
        assert_eq!(messages[2].content, "third");
    }

    #[tokio::test]
    async fn recent_messages_limit() {
        let pool = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");

        upsert_user(&pool, "@alice:example.org", Some("Alice"))
            .await
            .expect("upsert should succeed");

        for i in 1..=5 {
            save_message(
                &pool,
                "@alice:example.org",
                "!room:example.org",
                "user",
                &format!("message-{i}"),
            )
            .await
            .expect("save should succeed");
        }

        let messages = get_recent_messages(&pool, "!room:example.org", 2)
            .await
            .expect("read should succeed");

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "message-4");
        assert_eq!(messages[1].content, "message-5");
    }

    #[tokio::test]
    async fn upsert_user_creates_and_updates() {
        let pool = init_db("sqlite::memory:")
            .await
            .expect("db init should succeed");

        upsert_user(&pool, "@alice:example.org", Some("Alice One"))
            .await
            .expect("first upsert should succeed");
        upsert_user(&pool, "@alice:example.org", Some("Alice Two"))
            .await
            .expect("second upsert should succeed");

        let display_name: Option<String> =
            sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
                .bind("@alice:example.org")
                .fetch_one(&pool)
                .await
                .expect("query should succeed");

        assert_eq!(display_name.as_deref(), Some("Alice Two"));
    }
}
