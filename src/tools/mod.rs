pub mod memory;
pub mod remind;
pub mod time;

use std::collections::HashMap;

use async_trait::async_trait;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &str;
    async fn execute(&self, params: serde_json::Value) -> Result<String, String>;
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
        self.tools.insert(tool.name().to_owned(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
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

    pub fn build_tools_prompt(&self) -> String {
        let mut lines = vec!["available tools:".to_owned(), String::new()];

        for (name, description, params) in self.list() {
            lines.push(format!("[{name}]"));
            lines.push(format!("description: {description}"));
            lines.push(format!("params: {params}"));
            lines.push(String::new());
        }

        lines.join("\n")
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
    fn build_tools_prompt_includes_all() {
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

        let prompt = registry.build_tools_prompt();
        assert!(prompt.contains("[memory_read]"));
        assert!(prompt.contains("description: read memory"));
        assert!(prompt.contains("[current_time]"));
        assert!(prompt.contains("description: get time"));
    }
}
