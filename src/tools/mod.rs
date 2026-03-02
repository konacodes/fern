pub mod memory;
pub mod remind;
pub mod time;

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::json;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &str;
    async fn execute(&self, params: serde_json::Value) -> Result<String, String>;

    fn tool_schema(&self) -> serde_json::Value {
        let parameters = if self.parameters() == "none" {
            json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            })
        } else {
            json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": true
            })
        };

        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "strict": false,
                "description": self.description(),
                "parameters": parameters
            }
        })
    }
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        tracing::info!(tool = %tool.name(), "registering tool");
        self.tools.insert(tool.name().to_owned(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        let found = self.tools.contains_key(name);
        tracing::debug!(tool = name, found, "tool lookup");
        self.tools.get(name).map(|tool| tool.as_ref())
    }

    pub fn list(&self) -> Vec<(&str, &str, &str)> {
        let mut list = self
            .tools
            .values()
            .map(|tool| (tool.name(), tool.description(), tool.parameters()))
            .collect::<Vec<_>>();
        list.sort_unstable_by_key(|(name, _, _)| *name);
        list
    }

    pub fn build_tools_schema(&self) -> Vec<serde_json::Value> {
        let mut tools = self
            .tools
            .values()
            .map(|tool| tool.tool_schema())
            .collect::<Vec<_>>();
        tools.sort_by_key(|tool| {
            tool.get("function")
                .and_then(|func| func.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned()
        });
        tracing::debug!(
            tools_count = tools.len(),
            tool_names = %tools
                .iter()
                .filter_map(|tool| {
                    tool.get("function")
                        .and_then(|func| func.get("name"))
                        .and_then(serde_json::Value::as_str)
                })
                .collect::<Vec<_>>()
                .join(","),
            "built tools schema"
        );
        tools
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::Value;

    use super::{Tool, ToolRegistry};

    struct DummyTool {
        name: &'static str,
        description: &'static str,
        parameters: &'static str,
    }

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.description
        }

        fn parameters(&self) -> &str {
            self.parameters
        }

        async fn execute(&self, _params: Value) -> Result<String, String> {
            Ok("ok".to_owned())
        }
    }

    #[test]
    fn register_and_get_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "dummy",
            description: "dummy description",
            parameters: "none",
        }));

        let tool = registry.get("dummy");
        assert!(tool.is_some());
        assert_eq!(tool.expect("tool should exist").name(), "dummy");
    }

    #[test]
    fn get_missing_tool() {
        let registry = ToolRegistry::new();
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn list_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "tool_one",
            description: "first",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "tool_two",
            description: "second",
            parameters: "id (string)",
        }));

        let list = registry.list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|(name, _, _)| *name == "tool_one"));
        assert!(list.iter().any(|(name, _, _)| *name == "tool_two"));
    }

    #[test]
    fn build_tools_schema_includes_all() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "memory_read",
            description: "read memory",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "current_time",
            description: "get time",
            parameters: "none",
        }));

        let schema = registry.build_tools_schema();
        let schema_text =
            serde_json::to_string(&schema).expect("tools schema should serialize to json");
        assert!(schema_text.contains("\"name\":\"memory_read\""));
        assert!(schema_text.contains("\"description\":\"read memory\""));
        assert!(schema_text.contains("\"name\":\"current_time\""));
        assert!(schema_text.contains("\"description\":\"get time\""));
    }
}
