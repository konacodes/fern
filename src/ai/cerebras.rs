use serde::{Deserialize, Serialize};

use crate::Config;

pub type AiResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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

    pub async fn chat(&self, system: &str, messages: Vec<ChatMessage>) -> AiResult<String> {
        let mut all_messages = Vec::with_capacity(messages.len() + 1);
        all_messages.push(ChatMessage {
            role: "system".to_owned(),
            content: system.to_owned(),
        });
        all_messages.extend(messages);

        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages: all_messages,
            max_tokens: 512,
            temperature: 0.7,
        };

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
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Cerebras API returned HTTP {}: {body}", status.as_u16()).into());
        }

        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|err| format!("failed to parse Cerebras response JSON: {err}"))?;

        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .ok_or_else(|| "Cerebras response missing choices[0].message.content".into())
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponseMessage {
    content: Option<String>,
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

        let _ = client
            .chat(
                "system prompt",
                vec![ChatMessage {
                    role: "user".to_owned(),
                    content: "hello fern".to_owned(),
                }],
            )
            .await
            .expect("chat should succeed");
    }

    #[tokio::test]
    async fn cerebras_parses_response() {
        let server = MockServer::start().await;
        let config = test_config(format!("{}/v1", server.uri()));
        let client = CerebrasClient::new(&config);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "fern reply" } }]
            })))
            .mount(&server)
            .await;

        let text = client
            .chat("system", vec![])
            .await
            .expect("chat should parse response");
        assert_eq!(text, "fern reply");
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
            .chat("system", vec![])
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
            .chat("system", vec![])
            .await
            .expect_err("chat should fail on malformed JSON")
            .to_string();

        assert!(
            err.contains("parse") || err.contains("JSON"),
            "expected parse error message, got: {err}"
        );
    }
}
