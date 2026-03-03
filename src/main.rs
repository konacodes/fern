use std::sync::{Arc, RwLock};

use fern::{
    ai::{anthropic::AnthropicClient, cerebras::CerebrasClient},
    db,
    memory::consolidator::{run_nightly_loop, Consolidator},
    orchestrator::engine::Orchestrator,
    tools::{
        generator::ToolGenerator,
        loader::load_and_register_tools,
        memory::{MemoryReadTool, MemoryWriteTool},
        remind::{run_reminder_loop, RemindTool, ReminderStore},
        request_tool::RequestToolTool,
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
    tracing::info!(
        homeserver_url = %config.homeserver_url,
        cerebras_base_url = %config.cerebras_base_url,
        configured_model = %config.cerebras_model,
        database_url = %config.database_url,
        data_dir = %config.data_dir,
        "loaded fern configuration"
    );
    std::fs::create_dir_all(&config.data_dir)?;
    let db = db::init_db(&config.database_url).await?;

    let mut orchestrator_config = config.clone();
    orchestrator_config.cerebras_model = "gpt-oss-120b".to_owned();
    let orchestrator_cerebras = Arc::new(CerebrasClient::new(&orchestrator_config));
    let consolidator_cerebras = Arc::new(CerebrasClient::new(&config));
    let reminder_store = ReminderStore::new();

    let mut registry = ToolRegistry::new();
    registry.register(Box::new(MemoryReadTool::new(config.data_dir.clone())));
    registry.register(Box::new(MemoryWriteTool::new(config.data_dir.clone())));
    registry.register(Box::new(CurrentTimeTool));
    registry.register(Box::new(RemindTool::new(reminder_store.clone())));
    load_and_register_tools(&config.data_dir, &mut registry);
    let registry = Arc::new(RwLock::new(registry));
    tracing::info!("tool registry initialized with static + dynamic tools");

    let anthropic = config.anthropic_api_key.as_ref().map(|api_key| {
        Arc::new(AnthropicClient::new(
            api_key.clone(),
            config.anthropic_model.clone(),
        ))
    });
    if let Some(anthropic) = anthropic {
        tracing::info!(model = %config.anthropic_model, "anthropic tool generation enabled");
        let generator = Arc::new(ToolGenerator::new(anthropic, config.data_dir.clone()));
        let request_tool =
            RequestToolTool::new(generator, Arc::clone(&registry), config.data_dir.clone());
        let mut guard = registry
            .write()
            .map_err(|_| std::io::Error::other("failed to acquire tool registry write lock"))?;
        guard.register(Box::new(request_tool));
    } else {
        tracing::info!("anthropic api key not set; request_tool is disabled");
    }

    let orchestrator = Arc::new(Orchestrator::new(
        Arc::clone(&orchestrator_cerebras),
        Arc::clone(&registry),
        config.data_dir.clone(),
        db.clone(),
    ));
    tracing::info!(model = "gpt-oss-120b", "orchestrator initialized");
    let consolidator = Arc::new(Consolidator::new(
        Arc::clone(&consolidator_cerebras),
        db.clone(),
        config.data_dir.clone(),
    ));
    tracing::info!(model = %config.cerebras_model, "consolidator initialized");

    tokio::spawn(run_nightly_loop(Arc::clone(&consolidator)));
    tracing::info!("spawned nightly consolidation loop");

    let bot = FernBot::new(config, orchestrator).await?;
    tokio::spawn(run_reminder_loop(
        reminder_store,
        bot.client.clone(),
        Arc::clone(&orchestrator_cerebras),
    ));
    tracing::info!("spawned reminder loop");
    bot.run().await?;
    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("fern=trace,matrix_sdk=info,sqlx=warn,reqwest=warn"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}
