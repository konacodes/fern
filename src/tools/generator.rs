use std::sync::Arc;

use crate::{
    ai::anthropic::AnthropicClient,
    tools::{
        dynamic::{DynamicToolDef, DynamicToolType},
        script_tool::validate_script_source,
    },
};

pub const TOOL_GENERATION_PROMPT: &str = r#"you are a tool designer for fern, a personal assistant chatbot. you create tools that fern can use to interact with the world.

when given a description of what's needed, you design a tool and respond with ONLY a JSON object (no markdown, no explanation, no code fences).

you can create two types of tools:

TYPE 1 — HTTP tool (for calling APIs):
{
  "name": "tool_name_snake_case",
  "description": "one-line description of what this tool does",
  "parameters": [
    { "name": "param_name", "param_type": "string", "description": "what this param is", "required": true }
  ],
  "tool_type": {
    "type": "Http",
    "url_template": "https://api.example.com/endpoint?q={{param_name}}",
    "method": "GET",
    "headers": { "Accept": "application/json" },
    "body_template": null,
    "response_jq": ".path.to.useful.data"
  }
}

TYPE 2 — Script tool (for local computation):
{
  "name": "tool_name_snake_case",
  "description": "one-line description",
  "parameters": [
    { "name": "param_name", "param_type": "string", "description": "what this param is", "required": true }
  ],
  "tool_type": {
    "type": "Script",
    "interpreter": "python3",
    "source": "import sys, json\nparams = json.loads(sys.argv[1])\nprint(params['param_name'].upper())"
  }
}

rules:
- prefer HTTP tools when an API exists for the task (weather, search, etc.)
- use script tools for computation, text manipulation, or when no API exists
- tool names must be snake_case, lowercase, no spaces
- for HTTP tools, use free/no-auth APIs when possible (wttr.in, open-meteo, etc.)
- if an API requires a key, include an "api_key" parameter so fern can ask the user for it
- for script tools, the script MUST read params from sys.argv[1] as JSON
- scripts should be self-contained — no pip installs, only stdlib
- keep it minimal — one tool, one job
- response_jq uses dot notation: .foo.bar[0].baz
- url_template uses {{param_name}} for substitution (double curly braces)

respond with ONLY the JSON. nothing else."#;

pub struct ToolGenerator {
    anthropic: Arc<AnthropicClient>,
    data_dir: String,
}

impl ToolGenerator {
    pub fn new(anthropic: Arc<AnthropicClient>, data_dir: String) -> Self {
        Self {
            anthropic,
            data_dir,
        }
    }

    pub async fn generate_tool(&self, request: &str) -> Result<DynamicToolDef, String> {
        let raw = self
            .anthropic
            .complete(TOOL_GENERATION_PROMPT, request)
            .await
            .map_err(|err| format!("tool generation request failed: {err}"))?;
        let cleaned = strip_markdown_fences(&raw);
        let def = serde_json::from_str::<DynamicToolDef>(&cleaned)
            .map_err(|err| format!("failed parsing generated tool json: {err}"))?;
        validate_tool_def(&def)?;
        if let DynamicToolType::Script { source, .. } = &def.tool_type {
            validate_script_source(source)?;
        }
        def.save(&self.data_dir)?;
        Ok(def)
    }
}

pub fn validate_tool_def(def: &DynamicToolDef) -> Result<(), String> {
    let name_len = def.name.chars().count();
    if !(3..=50).contains(&name_len)
        || !def
            .name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err("invalid name: must be 3-50 chars and [a-z0-9_] only".to_owned());
    }

    let description = def.description.trim();
    if description.is_empty() || description.chars().count() >= 200 {
        return Err("invalid description: must be non-empty and under 200 chars".to_owned());
    }

    for param in &def.parameters {
        if param.name.trim().is_empty() {
            return Err("invalid parameter: name cannot be empty".to_owned());
        }
        if !matches!(
            param.param_type.as_str(),
            "string" | "integer" | "number" | "boolean"
        ) {
            return Err(format!("invalid parameter type: {}", param.param_type));
        }
        if param.description.trim().is_empty() {
            return Err("invalid parameter: description cannot be empty".to_owned());
        }
    }

    match &def.tool_type {
        DynamicToolType::Http {
            url_template,
            method,
            ..
        } => {
            if url_template.trim().is_empty() {
                return Err("invalid http tool: url_template cannot be empty".to_owned());
            }
            let method_upper = method.to_ascii_uppercase();
            if !matches!(
                method_upper.as_str(),
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
            ) {
                return Err(format!("invalid http method: {method}"));
            }
        }
        DynamicToolType::Script {
            interpreter,
            source,
        } => {
            if source.trim().is_empty() {
                return Err("invalid script tool: source cannot be empty".to_owned());
            }
            if interpreter != "python3" && interpreter != "bash" {
                return Err(format!(
                    "invalid script interpreter: {interpreter} (expected python3 or bash)"
                ));
            }
        }
    }

    Ok(())
}

fn strip_markdown_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_owned();
    }

    let mut lines = trimmed.lines().collect::<Vec<_>>();
    if lines.len() < 2 {
        return trimmed.to_owned();
    }
    if !lines[0].starts_with("```") {
        return trimmed.to_owned();
    }
    if let Some(last) = lines.last() {
        if last.trim() == "```" {
            let _ = lines.pop();
        }
    }
    let content = lines.into_iter().skip(1).collect::<Vec<_>>().join("\n");
    content.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use serde_json::json;
    use tempfile::tempdir;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::{
        ai::anthropic::AnthropicClient,
        tools::dynamic::{DynamicToolDef, DynamicToolType, ToolParam},
    };

    use super::{validate_tool_def, ToolGenerator, TOOL_GENERATION_PROMPT};

    fn sample_http_def() -> DynamicToolDef {
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

    fn sample_script_def() -> DynamicToolDef {
        DynamicToolDef {
            name: "uppercase_text".to_owned(),
            description: "uppercase provided text".to_owned(),
            parameters: vec![ToolParam {
                name: "text".to_owned(),
                param_type: "string".to_owned(),
                description: "text input".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Script {
                interpreter: "python3".to_owned(),
                source:
                    "import json,sys\nparams=json.loads(sys.argv[1])\nprint(params['text'].upper())"
                        .to_owned(),
            },
        }
    }

    #[test]
    fn validate_good_http_tool() {
        let def = sample_http_def();
        validate_tool_def(&def).expect("http def should be valid");
    }

    #[test]
    fn validate_good_script_tool() {
        let def = sample_script_def();
        validate_tool_def(&def).expect("script def should be valid");
    }

    #[test]
    fn validate_rejects_bad_name() {
        let mut def = sample_http_def();
        def.name = "weather lookup".to_owned();
        let err = validate_tool_def(&def).expect_err("bad name should be rejected");
        assert!(
            err.contains("name"),
            "expected name validation error, got: {err}"
        );
    }

    #[test]
    fn validate_rejects_empty_description() {
        let mut def = sample_http_def();
        def.description = "".to_owned();
        let err = validate_tool_def(&def).expect_err("empty description should be rejected");
        assert!(
            err.contains("description"),
            "expected description validation error, got: {err}"
        );
    }

    #[test]
    fn validate_rejects_bad_method() {
        let mut def = sample_http_def();
        if let DynamicToolType::Http { method, .. } = &mut def.tool_type {
            *method = "YOLO".to_owned();
        }
        let err = validate_tool_def(&def).expect_err("bad method should be rejected");
        assert!(
            err.contains("method"),
            "expected method validation error, got: {err}"
        );
    }

    #[test]
    fn validate_rejects_bad_interpreter() {
        let mut def = sample_script_def();
        if let DynamicToolType::Script { interpreter, .. } = &mut def.tool_type {
            *interpreter = "ruby".to_owned();
        }
        let err = validate_tool_def(&def).expect_err("bad interpreter should be rejected");
        assert!(
            err.contains("interpreter"),
            "expected interpreter validation error, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_tool_parses_claude_response() {
        let server = MockServer::start().await;
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let dir = tempdir().expect("tempdir should be created");
        let generator =
            ToolGenerator::new(client, dir.path().to_str().expect("utf-8 path").to_owned());

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "anthropic-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&sample_http_def()).expect("serialize tool")
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let def = generator
            .generate_tool("i need a weather tool")
            .await
            .expect("generation should succeed");
        assert_eq!(def.name, "weather_lookup");
        assert!(dir.path().join("tools/weather_lookup.json").exists());
        assert!(
            TOOL_GENERATION_PROMPT.contains("respond with ONLY the JSON"),
            "prompt should include strict output instruction"
        );
    }

    #[tokio::test]
    async fn generate_tool_handles_bad_json() {
        let server = MockServer::start().await;
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let dir = tempdir().expect("tempdir should be created");
        let generator =
            ToolGenerator::new(client, dir.path().to_str().expect("utf-8 path").to_owned());

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

        let err = generator
            .generate_tool("i need a weather tool")
            .await
            .expect_err("generation should fail");
        assert!(
            err.to_lowercase().contains("json"),
            "expected json parse error, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_tool_handles_markdown_wrapped() {
        let server = MockServer::start().await;
        let client = Arc::new(AnthropicClient::with_base_url(
            "anthropic-key",
            "claude-sonnet-4-20250514",
            server.uri(),
        ));
        let dir = tempdir().expect("tempdir should be created");
        let generator =
            ToolGenerator::new(client, dir.path().to_str().expect("utf-8 path").to_owned());
        let wrapped = format!(
            "```json\n{}\n```",
            serde_json::to_string(&sample_script_def()).expect("serialize tool")
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{
                    "type": "text",
                    "text": wrapped
                }]
            })))
            .mount(&server)
            .await;

        let def = generator
            .generate_tool("i need uppercase tool")
            .await
            .expect("generation should succeed");
        assert_eq!(def.name, "uppercase_text");
    }
}
