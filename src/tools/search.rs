use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::json;

use crate::tools::{Tool, ToolRegistry};

pub struct SearchToolsTool {
    registry: Arc<RwLock<ToolRegistry>>,
}

impl SearchToolsTool {
    pub fn new(registry: Arc<RwLock<ToolRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for SearchToolsTool {
    fn name(&self) -> &str {
        "search_tools"
    }

    fn description(&self) -> &str {
        "search for available tools by keyword. use this BEFORE calling a tool you haven't used recently — it tells you what's available. returns tool names and descriptions."
    }

    fn parameters(&self) -> &str {
        "query (string): keywords describing what you need, e.g. 'weather forecast' or 'news headlines'"
    }

    fn tool_schema(&self) -> serde_json::Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "strict": true,
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "keywords describing the needed capability"
                        }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let query = params
            .get("query")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| "missing required param: query".to_owned())?;

        let guard = self
            .registry
            .read()
            .map_err(|_| "failed to acquire tool registry read lock".to_owned())?;
        let results = guard
            .search(query)
            .into_iter()
            .map(|(name, description)| (name.to_owned(), description.to_owned()))
            .collect::<Vec<_>>();

        if results.is_empty() {
            return Ok(format!(
                "no tools found matching '{query}'. you can use request_tool to create one."
            ));
        }

        let mut output = format!("found {} tools:", results.len());
        for (name, description) in results {
            output.push_str(&format!("\n- {name}: {description}"));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use async_trait::async_trait;
    use serde_json::Value;

    use crate::tools::{Tool, ToolRegistry};

    use super::SearchToolsTool;

    struct DummyTool {
        name: &'static str,
        description: &'static str,
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
            "none"
        }

        async fn execute(&self, _params: Value) -> Result<String, String> {
            Ok("ok".to_owned())
        }
    }

    #[tokio::test]
    async fn search_tools_tool_formats_output() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool {
            name: "get_weather",
            description: "fetch current weather for a city using open-meteo",
        }));
        registry.register(Box::new(DummyTool {
            name: "weather_forecast",
            description: "get 5-day forecast for a location",
        }));
        let registry = Arc::new(RwLock::new(registry));
        let tool = SearchToolsTool::new(registry);

        let output = tool
            .execute(serde_json::json!({ "query": "weather forecast" }))
            .await
            .expect("execute should succeed");
        assert!(output.starts_with("found 2 tools:"));
        assert!(output.contains("- get_weather: fetch current weather for a city using open-meteo"));
        assert!(output.contains("- weather_forecast: get 5-day forecast for a location"));
    }

    #[tokio::test]
    async fn search_tools_tool_no_results_message() {
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let tool = SearchToolsTool::new(registry);

        let output = tool
            .execute(serde_json::json!({ "query": "xyzzy" }))
            .await
            .expect("execute should succeed");
        assert_eq!(
            output,
            "no tools found matching 'xyzzy'. you can use request_tool to create one."
        );
    }
}
