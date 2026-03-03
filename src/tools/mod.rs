pub mod delete;
pub mod dynamic;
pub mod generator;
pub mod http_tool;
pub mod improve;
pub mod loader;
pub mod memory;
pub mod personality;
pub mod remind;
pub mod request_tool;
pub mod script_tool;
pub mod search;
pub mod time;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    tools: HashMap<String, Arc<dyn Tool>>,
    builtin_names: HashSet<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            builtin_names: HashSet::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        tracing::info!(tool = %tool.name(), "registering tool");
        let arc: Arc<dyn Tool> = Arc::from(tool);
        self.tools.insert(arc.name().to_owned(), arc);
    }

    pub fn register_builtin(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_owned();
        self.register(tool);
        self.builtin_names.insert(name);
    }

    pub fn is_builtin(&self, name: &str) -> bool {
        self.builtin_names.contains(name)
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let found = self.tools.contains_key(name);
        tracing::debug!(tool = name, found, "tool lookup");
        self.tools.get(name).cloned()
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

    pub fn search(&self, query: &str) -> Vec<(&str, &str)> {
        let keywords = query
            .split_whitespace()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if keywords.is_empty() {
            return Vec::new();
        }

        let mut scored = self
            .tools
            .values()
            .filter_map(|tool| {
                let name = tool.name();
                let description = tool.description();
                let name_lower = name.to_ascii_lowercase();
                let description_lower = description.to_ascii_lowercase();
                let score = keywords.iter().fold(0usize, |acc, keyword| {
                    let mut score = acc;
                    if name_lower.contains(keyword) {
                        score += 2;
                    }
                    if description_lower.contains(keyword) {
                        score += 1;
                    }
                    score
                });
                (score > 0).then_some((score, name, description))
            })
            .collect::<Vec<_>>();

        scored.sort_unstable_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.cmp(right.1))
                .then_with(|| left.2.cmp(right.2))
        });
        scored
            .into_iter()
            .take(5)
            .map(|(_, name, description)| (name, description))
            .collect()
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

    pub fn get_always_available_schemas(&self) -> Vec<serde_json::Value> {
        let mut names = self.builtin_names.iter().collect::<Vec<_>>();
        names.sort_unstable();
        names
            .into_iter()
            .filter_map(|name| self.tools.get(name.as_str()))
            .map(|tool| tool.tool_schema())
            .collect()
    }

    pub fn get_schemas_by_names(&self, names: &[&str]) -> Vec<serde_json::Value> {
        let mut seen = HashSet::new();
        names
            .iter()
            .filter_map(|name| {
                if !seen.insert((*name).to_owned()) {
                    return None;
                }
                self.tools.get(*name).map(|tool| tool.tool_schema())
            })
            .collect()
    }

    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        if self.is_builtin(name) {
            return Err(format!("cannot remove built-in tool: {name}"));
        }
        if self.tools.remove(name).is_none() {
            return Err(format!("tool not found: {name}"));
        }
        self.builtin_names.remove(name);
        Ok(())
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

    #[test]
    fn search_matches_name() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "get_weather",
            description: "fetch forecast",
            parameters: "none",
        }));

        let results = registry.search("weather");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "get_weather");
    }

    #[test]
    fn search_matches_description() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "news_lookup",
            description: "fetch top headlines",
            parameters: "none",
        }));

        let results = registry.search("headlines");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "news_lookup");
    }

    #[test]
    fn search_no_match() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "memory_read",
            description: "read memory",
            parameters: "none",
        }));

        let results = registry.search("xyzzy");
        assert!(results.is_empty());
    }

    #[test]
    fn search_ranks_by_relevance() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "weather_lookup",
            description: "generic fetch",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "city_lookup",
            description: "weather by city",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "memory_read",
            description: "read memory",
            parameters: "none",
        }));

        let results = registry.search("weather");
        assert!(results.len() >= 2);
        assert_eq!(results[0].0, "weather_lookup");
        assert_eq!(results[1].0, "city_lookup");
    }

    #[test]
    fn always_available_includes_builtins() {
        let mut registry = ToolRegistry::new();
        registry.register_builtin(Box::new(DummyTool {
            name: "memory_read",
            description: "read memory",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "weather_lookup",
            description: "fetch weather",
            parameters: "none",
        }));

        let names = registry
            .get_always_available_schemas()
            .into_iter()
            .filter_map(|schema| {
                schema
                    .get("function")
                    .and_then(|value| value.get("name"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        assert!(names.iter().any(|name| name == "memory_read"));
        assert!(!names.iter().any(|name| name == "weather_lookup"));
    }

    #[test]
    fn get_schemas_by_names_returns_correct() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "tool_a",
            description: "a",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "tool_b",
            description: "b",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "tool_c",
            description: "c",
            parameters: "none",
        }));

        let names = registry
            .get_schemas_by_names(&["tool_a", "tool_c"])
            .into_iter()
            .filter_map(|schema| {
                schema
                    .get("function")
                    .and_then(|value| value.get("name"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|name| name == "tool_a"));
        assert!(names.iter().any(|name| name == "tool_c"));
        assert!(!names.iter().any(|name| name == "tool_b"));
    }

    #[test]
    fn get_schemas_by_names_skips_unknown() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "tool_a",
            description: "a",
            parameters: "none",
        }));

        let names = registry
            .get_schemas_by_names(&["missing"])
            .into_iter()
            .filter_map(|schema| {
                schema
                    .get("function")
                    .and_then(|value| value.get("name"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        assert!(names.is_empty());
    }

    #[test]
    fn registry_remove_method() {
        let mut registry = ToolRegistry::new();
        registry.register_builtin(Box::new(DummyTool {
            name: "memory_read",
            description: "read memory",
            parameters: "none",
        }));
        registry.register(Box::new(DummyTool {
            name: "weather_lookup",
            description: "fetch weather",
            parameters: "none",
        }));

        registry
            .remove("weather_lookup")
            .expect("dynamic tool removal should succeed");
        assert!(registry.get("weather_lookup").is_none());

        let builtin_err = registry
            .remove("memory_read")
            .expect_err("built-in removal should fail");
        assert!(builtin_err.contains("built-in"));

        let missing_err = registry
            .remove("missing_tool")
            .expect_err("missing tool removal should fail");
        assert!(missing_err.contains("not found"));
    }
}
