use std::sync::{Arc, RwLock};

use fern::Config;
use fern::{
    adapter::{orchestrator_handler::FernHandler, signal::SignalAdapter, MessagingAdapter},
    ai::{anthropic::AnthropicClient, cerebras::CerebrasClient},
    db,
    memory::consolidator::{run_nightly_loop, Consolidator},
    orchestrator::engine::Orchestrator,
    tools::{
        delete::DeleteToolTool,
        generator::ToolGenerator,
        improve::ImproveToolTool,
        loader::load_and_register_tools,
        memory::{MemoryReadTool, MemoryWriteTool},
        personality::{
            BehaviorsReadTool, BehaviorsWriteTool, PersonalityReadTool, PersonalityWriteTool,
        },
        remind::{run_reminder_loop, RemindTool, ReminderStore},
        request_tool::RequestToolTool,
        search::SearchToolsTool,
        time::CurrentTimeTool,
        ToolRegistry,
    },
};
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
        signal_api_url = %config.signal_api_url,
        signal_account_number = %config.signal_account_number,
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

    let anthropic = config.anthropic_api_key.as_ref().map(|api_key| {
        Arc::new(AnthropicClient::new(
            api_key.clone(),
            config.anthropic_model.clone(),
        ))
    });
    let generator =
        anthropic.map(|client| Arc::new(ToolGenerator::new(client, config.data_dir.clone())));
    if generator.is_some() {
        tracing::info!(model = %config.anthropic_model, "anthropic tool generation enabled");
    } else {
        tracing::info!("anthropic api key not set; request_tool is disabled");
    }
    let registry =
        build_registry_for_test(config.data_dir.clone(), reminder_store.clone(), generator)?;
    tracing::info!("tool registry initialized with static + dynamic tools");

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

    let adapter: Arc<dyn MessagingAdapter> = Arc::new(SignalAdapter::new(
        config.signal_api_url.clone(),
        config.signal_account_number.clone(),
    ));
    let handler = Arc::new(FernHandler::new(
        Arc::clone(&orchestrator),
        Arc::clone(&adapter),
        config.data_dir.clone(),
        db.clone(),
    ));
    tokio::spawn(run_reminder_loop(
        reminder_store,
        Arc::clone(&adapter),
        Arc::clone(&orchestrator_cerebras),
    ));
    tracing::info!("spawned reminder loop");
    adapter.run(handler).await.map_err(std::io::Error::other)?;
    Ok(())
}

fn build_registry_for_test(
    data_dir: String,
    reminder_store: ReminderStore,
    generator: Option<Arc<ToolGenerator>>,
) -> Result<Arc<RwLock<ToolRegistry>>, Box<dyn std::error::Error + Send + Sync>> {
    let mut registry = ToolRegistry::new();
    registry.register_builtin(Box::new(MemoryReadTool::new(data_dir.clone())));
    registry.register_builtin(Box::new(MemoryWriteTool::new(data_dir.clone())));
    registry.register_builtin(Box::new(PersonalityReadTool::new(data_dir.clone())));
    registry.register_builtin(Box::new(PersonalityWriteTool::new(data_dir.clone())));
    registry.register_builtin(Box::new(BehaviorsReadTool::new(data_dir.clone())));
    registry.register_builtin(Box::new(BehaviorsWriteTool::new(data_dir.clone())));
    registry.register_builtin(Box::new(CurrentTimeTool));
    registry.register_builtin(Box::new(RemindTool::new(reminder_store)));
    load_and_register_tools(&data_dir, &mut registry);
    let registry = Arc::new(RwLock::new(registry));

    {
        let mut guard = registry
            .write()
            .map_err(|_| std::io::Error::other("failed to acquire tool registry write lock"))?;
        guard.register_builtin(Box::new(SearchToolsTool::new(Arc::clone(&registry))));
        guard.register_builtin(Box::new(DeleteToolTool::new(
            Arc::clone(&registry),
            data_dir.clone(),
        )));
    }

    if let Some(generator) = generator {
        let mut guard = registry
            .write()
            .map_err(|_| std::io::Error::other("failed to acquire tool registry write lock"))?;
        guard.register_builtin(Box::new(RequestToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.clone(),
        )));
        guard.register_builtin(Box::new(ImproveToolTool::new(
            generator,
            Arc::clone(&registry),
            data_dir,
        )));
    }

    Ok(registry)
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("fern=trace,sqlx=warn,reqwest=warn"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::tempdir;

    use fern::{
        ai::anthropic::AnthropicClient,
        tools::{generator::ToolGenerator, remind::ReminderStore},
    };

    use super::build_registry_for_test;

    #[test]
    fn all_new_tools_registered() {
        let dir = tempdir().expect("tempdir should be created");
        let anthropic = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            "http://localhost",
        ));
        let generator = Arc::new(ToolGenerator::new(
            anthropic,
            dir.path().to_string_lossy().to_string(),
        ));

        let registry = build_registry_for_test(
            dir.path().to_string_lossy().to_string(),
            ReminderStore::new(),
            Some(generator),
        )
        .expect("registry should build");
        let guard = registry
            .read()
            .expect("registry lock should not be poisoned");

        for name in [
            "personality_read",
            "personality_write",
            "behaviors_read",
            "behaviors_write",
            "search_tools",
            "improve_tool",
            "delete_tool",
            "request_tool",
        ] {
            assert!(
                guard.get(name).is_some(),
                "expected tool to be registered: {name}"
            );
        }
    }

    #[test]
    fn new_tools_are_builtin() {
        let dir = tempdir().expect("tempdir should be created");
        let anthropic = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            "http://localhost",
        ));
        let generator = Arc::new(ToolGenerator::new(
            anthropic,
            dir.path().to_string_lossy().to_string(),
        ));

        let registry = build_registry_for_test(
            dir.path().to_string_lossy().to_string(),
            ReminderStore::new(),
            Some(generator),
        )
        .expect("registry should build");
        let guard = registry
            .read()
            .expect("registry lock should not be poisoned");

        for name in [
            "search_tools",
            "improve_tool",
            "delete_tool",
            "personality_read",
            "personality_write",
            "behaviors_read",
            "behaviors_write",
        ] {
            assert!(guard.is_builtin(name), "expected built-in tool: {name}");
        }
    }
}
