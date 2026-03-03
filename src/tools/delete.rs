use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::Value;

use crate::tools::{dynamic::delete_tool, Tool, ToolRegistry};

pub struct DeleteToolTool {
    registry: Arc<RwLock<ToolRegistry>>,
    data_dir: String,
}

impl DeleteToolTool {
    pub fn new(registry: Arc<RwLock<ToolRegistry>>, data_dir: String) -> Self {
        Self { registry, data_dir }
    }
}

#[async_trait]
impl Tool for DeleteToolTool {
    fn name(&self) -> &str {
        "delete_tool"
    }

    fn description(&self) -> &str {
        "delete a dynamic tool that is no longer useful. cannot delete built-in tools."
    }

    fn parameters(&self) -> &str {
        "tool_name (string): name of the tool to delete"
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
                            "description": "name of the dynamic tool to delete"
                        }
                    },
                    "required": ["tool_name"],
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

        {
            let mut registry = self
                .registry
                .write()
                .map_err(|_| "failed to acquire tool registry write lock".to_owned())?;
            registry.remove(tool_name)?;
        }

        delete_tool(&self.data_dir, tool_name)?;
        Ok(format!("deleted tool '{tool_name}'"))
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

    use crate::tools::{
        delete::DeleteToolTool,
        dynamic::{DynamicToolDef, DynamicToolType, ToolParam},
        http_tool::HttpTool,
        Tool, ToolRegistry,
    };

    fn sample_dynamic_tool() -> DynamicToolDef {
        DynamicToolDef {
            name: "weather_lookup".to_owned(),
            description: "fetch weather".to_owned(),
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
        let def = sample_dynamic_tool();
        def.save(data_dir).expect("tool should be saved");
        let tool = HttpTool::new(def).expect("http tool should build");
        registry
            .write()
            .expect("registry lock should not be poisoned")
            .register(Box::new(tool));
    }

    #[tokio::test]
    async fn delete_tool_removes_from_registry() {
        let dir = tempdir().expect("tempdir should be created");
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        setup_dynamic_tool(dir.path().to_str().expect("utf-8 path"), &registry);
        let tool = DeleteToolTool::new(
            Arc::clone(&registry),
            dir.path().to_string_lossy().to_string(),
        );

        let _ = tool
            .execute(json!({ "tool_name": "weather_lookup" }))
            .await
            .expect("delete should succeed");
        assert!(registry
            .read()
            .expect("registry lock should not be poisoned")
            .get("weather_lookup")
            .is_none());
    }

    #[tokio::test]
    async fn delete_tool_removes_from_disk() {
        let dir = tempdir().expect("tempdir should be created");
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        setup_dynamic_tool(dir.path().to_str().expect("utf-8 path"), &registry);
        let tool = DeleteToolTool::new(
            Arc::clone(&registry),
            dir.path().to_string_lossy().to_string(),
        );

        let _ = tool
            .execute(json!({ "tool_name": "weather_lookup" }))
            .await
            .expect("delete should succeed");
        assert!(!dir.path().join("tools/weather_lookup.json").exists());
    }

    #[tokio::test]
    async fn delete_tool_rejects_builtin() {
        let dir = tempdir().expect("tempdir should be created");
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        registry
            .write()
            .expect("registry lock should not be poisoned")
            .register_builtin(Box::new(BuiltinTool));
        let tool = DeleteToolTool::new(
            Arc::clone(&registry),
            dir.path().to_string_lossy().to_string(),
        );

        let err = tool
            .execute(json!({ "tool_name": "memory_read" }))
            .await
            .expect_err("built-in delete should fail");
        assert!(err.contains("built-in"));
    }

    #[tokio::test]
    async fn delete_tool_rejects_missing() {
        let dir = tempdir().expect("tempdir should be created");
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let tool = DeleteToolTool::new(
            Arc::clone(&registry),
            dir.path().to_string_lossy().to_string(),
        );

        let err = tool
            .execute(json!({ "tool_name": "missing_tool" }))
            .await
            .expect_err("missing delete should fail");
        assert!(err.contains("not found"));
    }
}
