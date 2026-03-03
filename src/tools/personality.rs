use async_trait::async_trait;
use serde_json::json;

use crate::{
    memory::{read_behaviors, read_personality, write_behaviors, write_personality},
    tools::Tool,
};

pub struct PersonalityReadTool {
    data_dir: String,
}

impl PersonalityReadTool {
    pub fn new(data_dir: String) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for PersonalityReadTool {
    fn name(&self) -> &str {
        "personality_read"
    }

    fn description(&self) -> &str {
        "read fern's personality file to see your current voice, values, and character"
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
        Ok(read_personality(&self.data_dir))
    }
}

pub struct PersonalityWriteTool {
    data_dir: String,
}

impl PersonalityWriteTool {
    pub fn new(data_dir: String) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for PersonalityWriteTool {
    fn name(&self) -> &str {
        "personality_write"
    }

    fn description(&self) -> &str {
        "update fern's personality. use this when you want to evolve how you present yourself — your voice, tone, values. send the COMPLETE updated file."
    }

    fn parameters(&self) -> &str {
        "content (string): the full updated personality.md content — must start with '# Fern's Personality'"
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
                        "content": {
                            "type": "string",
                            "description": "the full updated personality.md content"
                        }
                    },
                    "required": ["content"],
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let content = params
            .get("content")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing required param: content".to_owned())?;
        write_personality(&self.data_dir, content).map_err(|err| err.to_string())?;
        Ok("personality updated".to_owned())
    }
}

pub struct BehaviorsReadTool {
    data_dir: String,
}

impl BehaviorsReadTool {
    pub fn new(data_dir: String) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for BehaviorsReadTool {
    fn name(&self) -> &str {
        "behaviors_read"
    }

    fn description(&self) -> &str {
        "read fern's learned behaviors file to see operational patterns you've picked up"
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
        Ok(read_behaviors(&self.data_dir))
    }
}

pub struct BehaviorsWriteTool {
    data_dir: String,
}

impl BehaviorsWriteTool {
    pub fn new(data_dir: String) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for BehaviorsWriteTool {
    fn name(&self) -> &str {
        "behaviors_write"
    }

    fn description(&self) -> &str {
        "update fern's learned behaviors. use this when you figure out a better way to handle something — tool usage patterns, user preferences, workflow improvements. send the COMPLETE updated file."
    }

    fn parameters(&self) -> &str {
        "content (string): the full updated behaviors.md content — must start with '# Fern's Learned Behaviors'"
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
                        "content": {
                            "type": "string",
                            "description": "the full updated behaviors.md content"
                        }
                    },
                    "required": ["content"],
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let content = params
            .get("content")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing required param: content".to_owned())?;
        write_behaviors(&self.data_dir, content).map_err(|err| err.to_string())?;
        Ok("behaviors updated".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::{
        memory::{read_behaviors, read_personality},
        tools::Tool,
    };

    use super::{BehaviorsReadTool, BehaviorsWriteTool, PersonalityReadTool, PersonalityWriteTool};

    #[tokio::test]
    async fn personality_read_tool_returns_content() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let expected = "# Fern's Personality\n\n## Voice\n- concise";
        std::fs::write(dir.path().join("personality.md"), expected)
            .expect("personality file should be written");

        let tool = PersonalityReadTool::new(data_dir);
        let output = tool
            .execute(serde_json::json!({}))
            .await
            .expect("execute should succeed");
        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn personality_write_tool_updates() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let updated = "# Fern's Personality\n\n## Voice\n- playful";
        let tool = PersonalityWriteTool::new(data_dir.clone());

        tool.execute(serde_json::json!({ "content": updated }))
            .await
            .expect("execute should succeed");

        assert_eq!(read_personality(&data_dir), updated);
    }

    #[tokio::test]
    async fn personality_write_tool_rejects_bad() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let tool = PersonalityWriteTool::new(data_dir);

        let err = tool
            .execute(serde_json::json!({ "content": "bad" }))
            .await
            .expect_err("execute should fail for invalid content");
        assert!(err.contains("Personality"));
    }

    #[tokio::test]
    async fn behaviors_read_tool_returns_content() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let expected = "# Fern's Learned Behaviors\n\n## General\n- cite sources";
        std::fs::write(dir.path().join("behaviors.md"), expected)
            .expect("behaviors file should be written");

        let tool = BehaviorsReadTool::new(data_dir);
        let output = tool
            .execute(serde_json::json!({}))
            .await
            .expect("execute should succeed");
        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn behaviors_write_tool_updates() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let updated = "# Fern's Learned Behaviors\n\n## Tool Usage\n- use search_tools first";
        let tool = BehaviorsWriteTool::new(data_dir.clone());

        tool.execute(serde_json::json!({ "content": updated }))
            .await
            .expect("execute should succeed");

        assert_eq!(read_behaviors(&data_dir), updated);
    }

    #[tokio::test]
    async fn behaviors_write_tool_rejects_bad() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let tool = BehaviorsWriteTool::new(data_dir);

        let err = tool
            .execute(serde_json::json!({ "content": "bad" }))
            .await
            .expect_err("execute should fail for invalid content");
        assert!(err.contains("Learned Behaviors"));
    }
}
