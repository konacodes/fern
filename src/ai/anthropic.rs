use serde::{Deserialize, Serialize};

pub type AnthropicResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.anthropic.com".to_owned(),
        }
    }

    pub fn with_base_url(
        api_key: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: base_url.into(),
        }
    }

    pub async fn complete(&self, system: &str, user_message: &str) -> AnthropicResult<String> {
        let endpoint = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let request_body = MessageRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            system: system.to_owned(),
            messages: vec![MessageInput {
                role: "user".to_owned(),
                content: user_message.to_owned(),
            }],
        };

        let response = self
            .http
            .post(endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|err| format!("failed to call Anthropic API: {err}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| format!("failed reading Anthropic response body: {err}"))?;
        if !status.is_success() {
            return Err(format!("Anthropic API returned HTTP {}: {body}", status.as_u16()).into());
        }

        let parsed = serde_json::from_str::<MessageResponse>(&body)
            .map_err(|err| format!("failed to parse Anthropic response JSON: {err}"))?;
        let text = parsed
            .content
            .into_iter()
            .find_map(|block| {
                if block.content_type == "text" {
                    block.text
                } else {
                    None
                }
            })
            .ok_or_else(|| "Anthropic response missing content[0].text".to_owned())?;
        Ok(text)
    }
}

#[derive(Debug, Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<MessageInput>,
}

#[derive(Debug, Serialize)]
struct MessageInput {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct MessageResponse {
    content: Vec<MessageContent>,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::{
        matchers::{body_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use super::AnthropicClient;

    #[tokio::test]
    async fn anthropic_request_format() {
        let server = MockServer::start().await;
        let client =
            AnthropicClient::with_base_url("test-key", "claude-sonnet-4-20250514", server.uri());
        let expected = json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "system": "system prompt",
            "messages": [
                {
                    "role": "user",
                    "content": "hello"
                }
            ]
        });

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(header("content-type", "application/json"))
            .and(body_json(expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{ "type": "text", "text": "hello back" }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let response = client
            .complete("system prompt", "hello")
            .await
            .expect("completion should succeed");
        assert_eq!(response, "hello back");
    }

    #[tokio::test]
    async fn anthropic_extracts_text() {
        let server = MockServer::start().await;
        let client =
            AnthropicClient::with_base_url("test-key", "claude-sonnet-4-20250514", server.uri());

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{ "type": "text", "text": "hello" }]
            })))
            .mount(&server)
            .await;

        let response = client
            .complete("system prompt", "hello")
            .await
            .expect("completion should succeed");
        assert_eq!(response, "hello");
    }

    #[tokio::test]
    async fn anthropic_handles_error() {
        let server = MockServer::start().await;
        let client =
            AnthropicClient::with_base_url("test-key", "claude-sonnet-4-20250514", server.uri());

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("upstream failed"))
            .mount(&server)
            .await;

        let err = client
            .complete("system prompt", "hello")
            .await
            .expect_err("completion should fail")
            .to_string();
        assert!(
            err.contains("500"),
            "expected HTTP status in error message, got: {err}"
        );
    }

    #[tokio::test]
    async fn anthropic_handles_malformed_json() {
        let server = MockServer::start().await;
        let client =
            AnthropicClient::with_base_url("test-key", "claude-sonnet-4-20250514", server.uri());

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string("this is not json"))
            .mount(&server)
            .await;

        let err = client
            .complete("system prompt", "hello")
            .await
            .expect_err("completion should fail")
            .to_string();
        assert!(
            err.contains("parse") || err.contains("json") || err.contains("JSON"),
            "expected parse/json error message, got: {err}"
        );
    }
}
