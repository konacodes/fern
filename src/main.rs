use std::sync::Arc;

use fern::{
    ai::cerebras::CerebrasClient,
    db,
    memory::consolidator::{run_nightly_loop, Consolidator},
    orchestrator::engine::Orchestrator,
    tools::{
        memory::{MemoryReadTool, MemoryWriteTool},
        remind::{run_reminder_loop, RemindTool, ReminderStore},
        time::CurrentTimeTool,
        ToolRegistry,
    },
};
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

    let cerebras = Arc::new(CerebrasClient::new(&config));
    let reminder_store = ReminderStore::new();

    let mut registry = ToolRegistry::new();
    registry.register(Box::new(MemoryReadTool::new(config.data_dir.clone())));
    registry.register(Box::new(MemoryWriteTool::new(config.data_dir.clone())));
    registry.register(Box::new(CurrentTimeTool));
    registry.register(Box::new(RemindTool::new(reminder_store.clone())));
    let registry = Arc::new(registry);

    let orchestrator = Arc::new(Orchestrator::new(
        Arc::clone(&cerebras),
        Arc::clone(&registry),
        config.data_dir.clone(),
        db.clone(),
    ));
    let consolidator = Arc::new(Consolidator::new(
        Arc::clone(&cerebras),
        db.clone(),
        config.data_dir.clone(),
    ));

    tokio::spawn(run_nightly_loop(Arc::clone(&consolidator)));

    let bot = FernBot::new(config, orchestrator).await?;
    tokio::spawn(run_reminder_loop(reminder_store, bot.client.clone()));
    bot.run().await?;
    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}
