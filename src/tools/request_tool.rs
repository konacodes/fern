use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::tools::{
    dynamic::DynamicToolType, generator::ToolGenerator, http_tool::HttpTool,
    script_tool::ScriptTool, Tool, ToolRegistry,
};

pub struct RequestToolTool {
    generator: Arc<ToolGenerator>,
    registry: Arc<RwLock<ToolRegistry>>,
    data_dir: String,
}

impl RequestToolTool {
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
impl Tool for RequestToolTool {
    fn name(&self) -> &str {
        "request_tool"
    }

    fn description(&self) -> &str {
        "request a new tool when you can't do something with your current tools. describe in detail what you need — what it should do, what inputs it takes, what output you expect. a new tool will be generated and made available to you."
    }

    fn parameters(&self) -> &str {
        "description (string): detailed description of the tool you need, including what it does, expected inputs, and desired output format"
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
                        "description": {
                            "type": "string",
                            "description": "detailed description of the requested tool"
                        }
                    },
                    "required": ["description"],
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let description = params
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing required param: description".to_owned())?;

        let def = self.generator.generate_tool(description).await?;
        let tool_name = def.name.clone();
        let tool_description = def.description.clone();

        let tool: Box<dyn Tool> = match &def.tool_type {
            DynamicToolType::Http { .. } => Box::new(HttpTool::new(def)?),
            DynamicToolType::Script { .. } => {
                Box::new(ScriptTool::new(def, self.data_dir.clone())?)
            }
        };

        let mut registry = self
            .registry
            .write()
            .map_err(|_| "failed to acquire tool registry write lock".to_owned())?;
        registry.register(tool);

        Ok(format!(
            "new tool '{tool_name}' created and ready to use: {tool_description}. you can now call it."
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use serde_json::json;
    use tempfile::tempdir;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::{
        ai::anthropic::AnthropicClient,
        tools::{
            dynamic::{DynamicToolDef, DynamicToolType, ToolParam},
            generator::ToolGenerator,
            request_tool::RequestToolTool,
            Tool, ToolRegistry,
        },
    };

    fn generated_http_tool() -> DynamicToolDef {
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
                headers: std::collections::HashMap::new(),
                body_template: None,
                response_jq: Some(".current_condition[0].temp_F".to_owned()),
            },
        }
    }

    #[tokio::test]
    async fn request_tool_generates_and_registers() {
        let server = MockServer::start().await;
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let data_dir = tempdir().expect("tempdir should be created");
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let tool = RequestToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&generated_http_tool()).expect("serialize tool")
                }]
            })))
            .mount(&server)
            .await;

        let _ = tool
            .execute(json!({ "description": "need weather lookup by city" }))
            .await
            .expect("request_tool should succeed");

        let has_tool = registry
            .read()
            .expect("registry lock should not be poisoned")
            .get("weather_lookup")
            .is_some();
        assert!(has_tool, "generated tool should be registered");
    }

    #[tokio::test]
    async fn request_tool_returns_success_message() {
        let server = MockServer::start().await;
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let data_dir = tempdir().expect("tempdir should be created");
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let tool = RequestToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&generated_http_tool()).expect("serialize tool")
                }]
            })))
            .mount(&server)
            .await;

        let message = tool
            .execute(json!({ "description": "need weather lookup by city" }))
            .await
            .expect("request_tool should succeed");
        assert!(
            message.contains("weather_lookup"),
            "success should mention generated tool: {message}"
        );
    }

    #[tokio::test]
    async fn request_tool_handles_generation_failure() {
        let server = MockServer::start().await;
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let data_dir = tempdir().expect("tempdir should be created");
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let tool = RequestToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": "{not-json"
                }]
            })))
            .mount(&server)
            .await;

        let err = tool
            .execute(json!({ "description": "need weather lookup by city" }))
            .await
            .expect_err("request_tool should fail cleanly");
        assert!(
            err.to_lowercase().contains("json"),
            "expected generation error, got: {err}"
        );
    }

    #[tokio::test]
    async fn request_tool_missing_description() {
        let server = MockServer::start().await;
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let data_dir = tempdir().expect("tempdir should be created");
        let generator = Arc::new(ToolGenerator::new(
            client,
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        ));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let tool = RequestToolTool::new(
            Arc::clone(&generator),
            Arc::clone(&registry),
            data_dir.path().to_str().expect("utf-8 path").to_owned(),
        );

        let err = tool
            .execute(json!({}))
            .await
            .expect_err("request_tool should fail for missing description");
        assert!(
            err.contains("description"),
            "expected missing description error, got: {err}"
        );
    }
}
