use std::sync::Arc;

use fern::{ai::cerebras::CerebrasClient, db, engine::conversation::ConversationEngine};
use fern::{Config, FernBot};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    init_tracing();

    if let Err(err) = run().await {
        tracing::error!(error = %err, "fern failed");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = Config::from_env();
    std::fs::create_dir_all(&config.data_dir)?;
    let db = db::init_db(&config.database_url).await?;
    let cerebras = CerebrasClient::new(&config);
    let engine = Arc::new(ConversationEngine::new(cerebras, db));
    let bot = FernBot::new(config, engine).await?;
    bot.run().await?;
    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}
