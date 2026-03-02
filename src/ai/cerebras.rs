use serde::{Deserialize, Serialize};

use crate::Config;

pub type AiResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    #[serde(default = "default_assistant_role")]
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

fn default_assistant_role() -> String {
    "assistant".to_owned()
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new("system", content)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_owned(),
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunctionCall,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatCompletionChoice>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatCompletionChoice {
    pub message: ChatMessage,
}

#[derive(Clone, Debug)]
pub struct CerebrasClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl CerebrasClient {
    pub fn new(config: &Config) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: config.cerebras_api_key.clone(),
            base_url: config.cerebras_base_url.clone(),
            model: config.cerebras_model.clone(),
        }
    }

    pub async fn chat(
        &self,
        system: &str,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> AiResult<ChatCompletionResponse> {
        let mut all_messages = Vec::with_capacity(messages.len() + 1);
        all_messages.push(ChatMessage::system(system));
        all_messages.extend(messages);
        let tools_count = tools.as_ref().map_or(0, Vec::len);

        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages: all_messages,
            max_tokens: 512,
            temperature: 0.7,
            parallel_tool_calls: tools.as_ref().map(|_| false),
            tools,
        };
        tracing::debug!(
            model = %self.model,
            message_count = request_body.messages.len(),
            tools_count,
            "sending cerebras chat request"
        );
        if let Ok(serialized) = serde_json::to_string(&request_body) {
            tracing::trace!(request = %truncate_for_log(&serialized, 4000), "cerebras request payload");
        }

        let endpoint = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(|err| format!("failed to call Cerebras API: {err}"))?;

        let status = response.status();
        tracing::debug!(
            model = %self.model,
            status = %status.as_u16(),
            "received cerebras chat response"
        );
        let body = response
            .text()
            .await
            .map_err(|err| format!("failed reading Cerebras response body: {err}"))?;
        tracing::trace!(
            response_body = %truncate_for_log(&body, 4000),
            "raw cerebras response body"
        );
        if !status.is_success() {
            return Err(format!("Cerebras API returned HTTP {}: {body}", status.as_u16()).into());
        }

        let parsed = serde_json::from_str::<ChatCompletionResponse>(&body).map_err(|err| {
            tracing::error!(
                error = %err,
                response_body = %truncate_for_log(&body, 1000),
                "failed to parse cerebras response json"
            );
            format!("failed to parse Cerebras response JSON: {err}")
        })?;

        let (tool_calls, has_content) = parsed
            .choices
            .first()
            .map(|choice| {
                (
                    choice
                        .message
                        .tool_calls
                        .as_ref()
                        .map_or(0, std::vec::Vec::len),
                    choice
                        .message
                        .content
                        .as_ref()
                        .map(|content| !content.trim().is_empty())
                        .unwrap_or(false),
                )
            })
            .unwrap_or((0, false));
        tracing::debug!(
            choices = parsed.choices.len(),
            first_tool_calls = tool_calls,
            first_has_content = has_content,
            "parsed cerebras chat response"
        );

        Ok(parsed)
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let truncated = value.chars().take(max_chars).collect::<String>();
    format!("{truncated}...[truncated]")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::{
        matchers::{body_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::Config;

    use super::{CerebrasClient, ChatMessage};

    fn test_config(base_url: String) -> Config {
        Config {
            homeserver_url: "http://localhost:6167".to_owned(),
            bot_user: "@fern:example.org".to_owned(),
            bot_password: "password".to_owned(),
            data_dir: "./data".to_owned(),
            cerebras_api_key: "test-key".to_owned(),
            cerebras_model: "qwen-3-235b".to_owned(),
            cerebras_base_url: base_url,
            database_url: "sqlite::memory:".to_owned(),
        }
    }

    #[tokio::test]
    async fn cerebras_request_format() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let client = CerebrasClient::new(&config);

        let expected = json!({
            "model": "qwen-3-235b",
            "messages": [
                { "role": "system", "content": "system prompt" },
                { "role": "user", "content": "hello fern" }
            ],
            "max_tokens": 512,
            "temperature": 0.7
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .and(header("content-type", "application/json"))
            .and(body_json(expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "hi there" } }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let response = client
            .chat(
                "system prompt",
                vec![ChatMessage::new("user", "hello fern")],
                None,
            )
            .await
            .expect("chat should succeed");
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("hi there")
        );
    }

    #[tokio::test]
    async fn cerebras_request_includes_tools() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let client = CerebrasClient::new(&config);

        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "current_time",
                "description": "get current date and time",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": [],
                    "additionalProperties": false
                }
            }
        })];

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_json(json!({
                "model": "qwen-3-235b",
                "messages": [
                    { "role": "system", "content": "system prompt" },
                    { "role": "user", "content": "what time is it?" }
                ],
                "max_tokens": 512,
                "temperature": 0.7,
                "parallel_tool_calls": false,
                "tools": tools
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "ok" } }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let _ = client
            .chat(
                "system prompt",
                vec![ChatMessage::new("user", "what time is it?")],
                Some(vec![json!({
                    "type": "function",
                    "function": {
                        "name": "current_time",
                        "description": "get current date and time",
                        "parameters": {
                            "type": "object",
                            "properties": {},
                            "required": [],
                            "additionalProperties": false
                        }
                    }
                })]),
            )
            .await
            .expect("chat should succeed");
    }

    #[tokio::test]
    async fn cerebras_parses_tool_calls() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let client = CerebrasClient::new(&config);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": {
                        "content": "let me check",
                        "tool_calls": [{
                            "id": "call_123",
                            "type": "function",
                            "function": {
                                "name": "current_time",
                                "arguments": "{}"
                            }
                        }]
                    }
                }]
            })))
            .mount(&server)
            .await;

        let response = client
            .chat(
                "system",
                vec![ChatMessage::new("user", "time?")],
                Some(vec![]),
            )
            .await
            .expect("chat should parse tool calls");
        let tool_calls = response.choices[0]
            .message
            .tool_calls
            .as_ref()
            .expect("tool calls should exist");
        assert_eq!(tool_calls[0].id, "call_123");
        assert_eq!(tool_calls[0].function.name, "current_time");
    }

    #[tokio::test]
    async fn cerebras_handles_error() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let client = CerebrasClient::new(&config);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("upstream failed"))
            .mount(&server)
            .await;

        let err = client
            .chat("system", vec![], None)
            .await
            .expect_err("chat should fail on http 500")
            .to_string();

        assert!(
            err.contains("500"),
            "expected status code in error message, got: {err}"
        );
    }

    #[tokio::test]
    async fn cerebras_handles_malformed_json() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let client = CerebrasClient::new(&config);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("this is not json"))
            .mount(&server)
            .await;

        let err = client
            .chat("system", vec![], None)
            .await
            .expect_err("chat should fail on malformed JSON")
            .to_string();

        assert!(
            err.contains("parse") || err.contains("JSON"),
            "expected parse error message, got: {err}"
        );
    }
}
