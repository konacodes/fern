use async_trait::async_trait;

use crate::{memory, tools::Tool};

pub struct MemoryReadTool {
    data_dir: String,
}

impl MemoryReadTool {
    pub fn new(data_dir: String) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for MemoryReadTool {
    fn name(&self) -> &str {
        "memory_read"
    }

    fn description(&self) -> &str {
        "read fern's memory file to recall what you know about the user"
    }

    fn parameters(&self) -> &str {
        "none"
    }

    async fn execute(&self, _params: serde_json::Value) -> Result<String, String> {
        Ok(memory::read_memory(&self.data_dir))
    }
}

pub struct MemoryWriteTool {
    data_dir: String,
}

impl MemoryWriteTool {
    pub fn new(data_dir: String) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for MemoryWriteTool {
    fn name(&self) -> &str {
        "memory_write"
    }

    fn description(&self) -> &str {
        "update fern's memory file. use this when you learn something new about the user worth remembering. send the COMPLETE updated file content."
    }

    fn parameters(&self) -> &str {
        "content (string): the full updated memory.md content — must start with '# Fern's Memory' and preserve the 4-section structure"
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let content = params
            .get("content")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "missing required param: content".to_owned())?;

        if !content.starts_with("# Fern's Memory") {
            return Err("content must start with '# Fern's Memory'".to_owned());
        }
        for heading in [
            "## Working Memory",
            "## Projects & Work",
            "## Preferences & Style",
            "## Long-Term Memory",
        ] {
            if !content.contains(heading) {
                return Err(format!("content must include section: {heading}"));
            }
        }

        memory::write_memory(&self.data_dir, content)
            .map_err(|err| format!("failed to write memory: {err}"))?;
        Ok("memory updated".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{MemoryReadTool, MemoryWriteTool};
    use crate::{
        memory::{read_memory, MEMORY_TEMPLATE},
        tools::Tool,
    };

    #[tokio::test]
    async fn memory_read_returns_content() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let expected = "# Fern's Memory\n\n## Working Memory\n- likes tea";
        std::fs::write(dir.path().join("memory.md"), expected)
            .expect("memory file should be written");

        let tool = MemoryReadTool::new(data_dir);
        let result = tool
            .execute(serde_json::json!({}))
            .await
            .expect("execute should succeed");
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn memory_read_creates_default() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let tool = MemoryReadTool::new(data_dir);
        let result = tool
            .execute(serde_json::json!({}))
            .await
            .expect("execute should succeed");
        assert_eq!(result, MEMORY_TEMPLATE);
    }

    #[tokio::test]
    async fn memory_write_updates_file() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let content = "# Fern's Memory

## Working Memory
- project deadline friday

## Projects & Work
- shipping fern phase 2

## Preferences & Style
- prefers concise responses

## Long-Term Memory
- has a wolf fursona named kona";

        let tool = MemoryWriteTool::new(data_dir.clone());
        tool.execute(serde_json::json!({ "content": content }))
            .await
            .expect("execute should succeed");

        let saved = read_memory(&data_dir);
        assert_eq!(saved, content);
    }

    #[tokio::test]
    async fn memory_write_rejects_invalid() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let tool = MemoryWriteTool::new(data_dir);

        let err = tool
            .execute(serde_json::json!({ "content": "invalid memory body" }))
            .await
            .expect_err("execute should return Err for invalid content");
        assert!(err.contains("must start with '# Fern's Memory'"));
    }
}
