use async_trait::async_trait;
use chrono::Local;
use serde_json::json;

use crate::tools::Tool;

pub struct CurrentTimeTool;

#[async_trait]
impl Tool for CurrentTimeTool {
    fn name(&self) -> &str {
        "current_time"
    }

    fn description(&self) -> &str {
        "get the current date, time, and day of the week"
    }

    fn parameters(&self) -> &str {
        "none"
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
                    "properties": {},
                    "required": [],
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, _params: serde_json::Value) -> Result<String, String> {
        let now = Local::now();
        Ok(now
            .format("%A, %B %-d, %Y at %-I:%M %p %Z")
            .to_string()
            .to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, Local};

    use super::CurrentTimeTool;
    use crate::tools::Tool;

    #[tokio::test]
    async fn current_time_returns_nonempty() {
        let tool = CurrentTimeTool;
        let result = tool
            .execute(serde_json::json!({}))
            .await
            .expect("execute should succeed");
        assert!(!result.trim().is_empty());
    }

    #[tokio::test]
    async fn current_time_contains_year() {
        let tool = CurrentTimeTool;
        let result = tool
            .execute(serde_json::json!({}))
            .await
            .expect("execute should succeed");
        let year = Local::now().year().to_string();
        assert!(result.contains(&year), "expected year {year} in {result}");
    }
}
