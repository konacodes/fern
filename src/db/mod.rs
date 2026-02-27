pub mod messages;

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

pub async fn init_db(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let max_connections = if database_url == "sqlite::memory:" {
        1
    } else {
        5
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            display_name TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (user_id) REFERENCES users(id)
        );",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_room_created
            ON messages(room_id, created_at);",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}
