use std::{
    path::Path,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use serde_json::Value;

use crate::tools::{
    dynamic::DynamicToolDef, generator::ToolGenerator, http_tool::HttpTool,
    script_tool::ScriptTool, Tool, ToolRegistry,
};

pub struct ImproveToolTool {
    generator: Arc<ToolGenerator>,
    registry: Arc<RwLock<ToolRegistry>>,
    data_dir: String,
}

impl ImproveToolTool {
    pub fn new(
        generator: Arc<ToolGenerator>,
        registry: Arc<RwLock<ToolRegistry>>,
        data_dir: String,
    ) -> Self {
        Self {
            generator,
            registry,
            data_dir,
        }
    }
}

#[async_trait]
impl Tool for ImproveToolTool {
    fn name(&self) -> &str {
        "improve_tool"
    }

    fn description(&self) -> &str {
        "improve an existing dynamic tool. use this when a tool returned bad results, failed, or doesn't do what you need. describe what went wrong and what you want instead."
    }

    fn parameters(&self) -> &str {
        "tool_name (string): name of the tool to improve, feedback (string): what went wrong and what you want changed"
    }

    fn tool_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "strict": true,
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tool_name": {
                            "type": "string",
                            "description": "name of the dynamic tool to improve"
                        },
                        "feedback": {
                            "type": "string",
                            "description": "what went wrong and what should change"
                        }
                    },
                    "required": ["tool_name", "feedback"],
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<String, String> {
        let tool_name = params
            .get("tool_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing required param: tool_name".to_owned())?;
        let feedback = params
            .get("feedback")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing required param: feedback".to_owned())?;

        {
            let registry = self
                .registry
                .read()
                .map_err(|_| "failed to acquire tool registry read lock".to_owned())?;
            if registry.is_builtin(tool_name) {
                return Err(format!("cannot improve built-in tool: {tool_name}"));
            }
            if registry.get(tool_name).is_none() {
                return Err(format!("tool not found: {tool_name}"));
            }
        }

        let path = Path::new(&self.data_dir)
            .join("tools")
            .join(format!("{tool_name}.json"));
        let existing_json = std::fs::read_to_string(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                format!("tool not found: {tool_name}")
            } else {
                format!(
                    "failed to read existing tool definition {}: {err}",
                    path.display()
                )
            }
        })?;
        let existing_def = serde_json::from_str::<DynamicToolDef>(&existing_json)
            .map_err(|err| format!("failed to parse existing tool definition: {err}"))?;

        let improved = self
            .generator
            .improve_tool(&existing_json, feedback)
            .await?;
        if improved.name != existing_def.name {
            return Err(format!(
                "improved tool must keep the same name: expected {}, got {}",
                existing_def.name, improved.name
            ));
        }
        improved.save(&self.data_dir)?;

        let tool: Box<dyn Tool> = match &improved.tool_type {
            crate::tools::dynamic::DynamicToolType::Http { .. } => {
                Box::new(HttpTool::new(improved.clone())?)
            }
            crate::tools::dynamic::DynamicToolType::Script { .. } => {
                Box::new(ScriptTool::new(improved.clone(), self.data_dir.clone())?)
            }
        };
        let mut registry = self
            .registry
            .write()
            .map_err(|_| "failed to acquire tool registry write lock".to_owned())?;
        registry.register(tool);

        Ok(format!(
            "tool '{tool_name}' improved and reloaded: {}",
            improved.description
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, RwLock},
    };

    use async_trait::async_trait;
    use serde_json::{json, Value};
    use tempfile::tempdir;
    use wiremock::{
        matchers::{body_string_contains, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::{
        ai::anthropic::AnthropicClient,
        tools::{
            dynamic::{DynamicToolDef, DynamicToolType, ToolParam},
            generator::ToolGenerator,
            http_tool::HttpTool,
            improve::ImproveToolTool,
            Tool, ToolRegistry,
        },
    };

    fn base_tool_def() -> DynamicToolDef {
        DynamicToolDef {
            name: "weather_lookup".to_owned(),
            description: "fetch weather for a city".to_owned(),
            parameters: vec![ToolParam {
                name: "location".to_owned(),
                param_type: "string".to_owned(),
                description: "city".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Http {
                url_template: "https://wttr.in/{{location}}?format=j1".to_owned(),
                method: "GET".to_owned(),
                headers: HashMap::new(),
                body_template: None,
                response_jq: Some(".current_condition[0].temp_F".to_owned()),
            },
        }
    }

    fn improved_tool_def() -> DynamicToolDef {
        DynamicToolDef {
            description: "fetch weather with richer details".to_owned(),
            ..base_tool_def()
        }
    }

    struct BuiltinTool;

    #[async_trait]
    impl Tool for BuiltinTool {
        fn name(&self) -> &str {
            "memory_read"
        }

        fn description(&self) -> &str {
            "read memory"
        }

        fn parameters(&self) -> &str {
            "none"
        }

        async fn execute(&self, _params: Value) -> Result<String, String> {
            Ok("ok".to_owned())
        }
    }

    fn setup_dynamic_tool(data_dir: &str, registry: &Arc<RwLock<ToolRegistry>>) {
        let def = base_tool_def();
        def.save(data_dir).expect("tool should be saved");
        let tool = HttpTool::new(def).expect("http tool should build");
        registry
            .write()
            .expect("registry lock should not be poisoned")
            .register(Box::new(tool));
    }

    #[tokio::test]
    async fn improve_tool_sends_to_claude() {
        let server = MockServer::start().await;
        let data_dir = tempdir().expect("tempdir should be created");
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_string_lossy().to_string(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        setup_dynamic_tool(data_dir.path().to_str().expect("utf-8 path"), &registry);
        let tool = ImproveToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_string_lossy().to_string(),
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(body_string_contains("weather_lookup"))
            .and(body_string_contains("failed to include humidity"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&improved_tool_def()).expect("serialize improved def")
                }]
            })))
            .mount(&server)
            .await;

        let _ = tool
            .execute(json!({
                "tool_name": "weather_lookup",
                "feedback": "failed to include humidity"
            }))
            .await
            .expect("improve should succeed");
    }

    #[tokio::test]
    async fn improve_tool_updates_registry() {
        let server = MockServer::start().await;
        let data_dir = tempdir().expect("tempdir should be created");
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_string_lossy().to_string(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        setup_dynamic_tool(data_dir.path().to_str().expect("utf-8 path"), &registry);
        let tool = ImproveToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_string_lossy().to_string(),
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&improved_tool_def()).expect("serialize improved def")
                }]
            })))
            .mount(&server)
            .await;

        let _ = tool
            .execute(json!({
                "tool_name": "weather_lookup",
                "feedback": "include humidity"
            }))
            .await
            .expect("improve should succeed");

        let description = registry
            .read()
            .expect("registry lock should not be poisoned")
            .get("weather_lookup")
            .expect("tool should exist")
            .description()
            .to_owned();
        assert_eq!(description, "fetch weather with richer details");
    }

    #[tokio::test]
    async fn improve_tool_updates_disk() {
        let server = MockServer::start().await;
        let data_dir = tempdir().expect("tempdir should be created");
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_string_lossy().to_string(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        setup_dynamic_tool(data_dir.path().to_str().expect("utf-8 path"), &registry);
        let tool = ImproveToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_string_lossy().to_string(),
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&improved_tool_def()).expect("serialize improved def")
                }]
            })))
            .mount(&server)
            .await;

        let _ = tool
            .execute(json!({
                "tool_name": "weather_lookup",
                "feedback": "include humidity"
            }))
            .await
            .expect("improve should succeed");

        let saved = std::fs::read_to_string(data_dir.path().join("tools/weather_lookup.json"))
            .expect("tool file should be readable");
        assert!(saved.contains("fetch weather with richer details"));
    }

    #[tokio::test]
    async fn improve_tool_rejects_builtin() {
        let server = MockServer::start().await;
        let data_dir = tempdir().expect("tempdir should be created");
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_string_lossy().to_string(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        registry
            .write()
            .expect("registry lock should not be poisoned")
            .register_builtin(Box::new(BuiltinTool));
        let tool = ImproveToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_string_lossy().to_string(),
        );

        let err = tool
            .execute(json!({
                "tool_name": "memory_read",
                "feedback": "do better"
            }))
            .await
            .expect_err("built-in tool improve should fail");
        assert!(err.contains("built-in"));
    }

    #[tokio::test]
    async fn improve_tool_rejects_missing() {
        let server = MockServer::start().await;
        let data_dir = tempdir().expect("tempdir should be created");
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_string_lossy().to_string(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let tool = ImproveToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_string_lossy().to_string(),
        );

        let err = tool
            .execute(json!({
                "tool_name": "missing_tool",
                "feedback": "do better"
            }))
            .await
            .expect_err("missing tool improve should fail");
        assert!(err.contains("not found"));
    }
}
