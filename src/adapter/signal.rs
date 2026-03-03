use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::adapter::{MessageHandler, MessagingAdapter};

pub struct SignalAdapter {
    pub api_url: String,
    pub account_number: String,
    pub http: reqwest::Client,
}

impl SignalAdapter {
    pub fn new(api_url: String, account_number: String) -> Self {
        Self {
            api_url,
            account_number,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl MessagingAdapter for SignalAdapter {
    async fn run(&self, handler: Arc<dyn MessageHandler>) -> Result<(), String> {
        let ws_base = websocket_base_url(&self.api_url);
        let ws_url = format!(
            "{}/v1/receive/{}",
            ws_base.trim_end_matches('/'),
            self.account_number
        );

        loop {
            match connect_async(&ws_url).await {
                Ok((stream, _response)) => {
                    tracing::info!(ws_url = %ws_url, "connected to signal websocket");
                    let (_write, mut read) = stream.split();
                    while let Some(frame_result) = read.next().await {
                        match frame_result {
                            Ok(Message::Text(text)) => {
                                if let Some((sender_id, conversation_id, message_text)) =
                                    parse_signal_envelope(&text, &self.account_number)
                                {
                                    match handler
                                        .handle_message(&sender_id, &conversation_id, &message_text)
                                        .await
                                    {
                                        Ok(response) => {
                                            if let Err(err) =
                                                self.send_message(&conversation_id, &response).await
                                            {
                                                tracing::error!(
                                                    error = %err,
                                                    conversation_id,
                                                    "failed sending signal response"
                                                );
                                            }
                                        }
                                        Err(err) => {
                                            tracing::error!(
                                                error = %err,
                                                sender_id,
                                                conversation_id,
                                                "message handler failed"
                                            );
                                        }
                                    }
                                } else {
                                    tracing::debug!("skipping non-message websocket envelope");
                                }
                            }
                            Ok(Message::Binary(bytes)) => match String::from_utf8(bytes) {
                                Ok(text) => {
                                    if let Some((sender_id, conversation_id, message_text)) =
                                        parse_signal_envelope(&text, &self.account_number)
                                    {
                                        match handler
                                            .handle_message(
                                                &sender_id,
                                                &conversation_id,
                                                &message_text,
                                            )
                                            .await
                                        {
                                            Ok(response) => {
                                                if let Err(err) = self
                                                    .send_message(&conversation_id, &response)
                                                    .await
                                                {
                                                    tracing::error!(
                                                        error = %err,
                                                        conversation_id,
                                                        "failed sending signal response"
                                                    );
                                                }
                                            }
                                            Err(err) => {
                                                tracing::error!(
                                                    error = %err,
                                                    sender_id,
                                                    conversation_id,
                                                    "message handler failed"
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    tracing::warn!(error = %err, "invalid utf-8 websocket frame")
                                }
                            },
                            Ok(Message::Close(_)) => {
                                tracing::warn!("signal websocket closed; reconnecting");
                                break;
                            }
                            Ok(_) => {}
                            Err(err) => {
                                tracing::error!(error = %err, "signal websocket read error");
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(error = %err, ws_url = %ws_url, "signal websocket connect failed");
                }
            }

            sleep(Duration::from_secs(5)).await;
            tracing::info!("retrying signal websocket connection");
        }
    }

    async fn send_message(&self, conversation_id: &str, text: &str) -> Result<(), String> {
        let endpoint = format!("{}/v2/send", self.api_url.trim_end_matches('/'));
        for chunk in split_message_chunks(text, 2000) {
            let payload = serde_json::json!({
                "message": chunk,
                "number": self.account_number,
                "recipients": [conversation_id]
            });
            let response = self
                .http
                .post(&endpoint)
                .json(&payload)
                .send()
                .await
                .map_err(|err| format!("failed to send signal message: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<unreadable body>".to_owned());
                return Err(format!(
                    "signal send API returned HTTP {}: {body}",
                    status.as_u16()
                ));
            }
        }
        Ok(())
    }
}

fn split_message_chunks(text: &str, max_len: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let chars = text.chars().collect::<Vec<_>>();
    chars
        .chunks(max_len)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

fn websocket_base_url(api_url: &str) -> String {
    if let Some(rest) = api_url.strip_prefix("https://") {
        return format!("wss://{rest}");
    }
    if let Some(rest) = api_url.strip_prefix("http://") {
        return format!("ws://{rest}");
    }
    api_url.to_owned()
}

fn parse_signal_envelope(payload: &str, account_number: &str) -> Option<(String, String, String)> {
    let frame = match serde_json::from_str::<SignalReceiveFrame>(payload) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = %err, payload, "failed parsing signal websocket frame json");
            return None;
        }
    };

    let envelope = frame.envelope?;
    let sender_id = envelope.source_number?;
    if sender_id == account_number {
        return None;
    }
    let data_message = envelope.data_message?;
    let message = data_message.message?;
    if message.trim().is_empty() {
        return None;
    }

    Some((sender_id.clone(), sender_id, message))
}

#[derive(Debug, Deserialize)]
struct SignalReceiveFrame {
    envelope: Option<SignalEnvelope>,
}

#[derive(Debug, Deserialize)]
struct SignalEnvelope {
    #[serde(rename = "sourceNumber")]
    source_number: Option<String>,
    #[serde(rename = "dataMessage")]
    data_message: Option<SignalDataMessage>,
}

#[derive(Debug, Deserialize)]
struct SignalDataMessage {
    message: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use wiremock::{
        matchers::{body_json, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::adapter::{signal::SignalAdapter, MessagingAdapter};

    #[test]
    fn parse_signal_envelope() {
        let payload = r#"{
          "envelope": {
            "sourceNumber": "+15559876543",
            "sourceName": "Jason",
            "dataMessage": {
              "message": "hey fern whats up",
              "timestamp": 1234567890
            }
          }
        }"#;
        let parsed = super::parse_signal_envelope(payload, "+15550000000");
        assert_eq!(
            parsed,
            Some((
                "+15559876543".to_owned(),
                "+15559876543".to_owned(),
                "hey fern whats up".to_owned(),
            ))
        );
    }

    #[test]
    fn parse_envelope_skips_typing() {
        let payload = r#"{
          "envelope": {
            "sourceNumber": "+15559876543",
            "sourceName": "Jason",
            "dataMessage": null
          }
        }"#;
        assert_eq!(super::parse_signal_envelope(payload, "+15550000000"), None);
    }

    #[test]
    fn parse_envelope_skips_self() {
        let payload = r#"{
          "envelope": {
            "sourceNumber": "+15550000000",
            "sourceName": "Fern",
            "dataMessage": {
              "message": "echo",
              "timestamp": 1234567890
            }
          }
        }"#;
        assert_eq!(super::parse_signal_envelope(payload, "+15550000000"), None);
    }

    #[test]
    fn parse_envelope_skips_empty_message() {
        let payload = r#"{
          "envelope": {
            "sourceNumber": "+15559876543",
            "sourceName": "Jason",
            "dataMessage": {
              "message": null,
              "timestamp": 1234567890
            }
          }
        }"#;
        assert_eq!(super::parse_signal_envelope(payload, "+15550000000"), None);
    }

    #[tokio::test]
    async fn send_message_posts_correctly() {
        let server = MockServer::start().await;
        let adapter = SignalAdapter::new(server.uri(), "+15550000000".to_owned());
        Mock::given(method("POST"))
            .and(path("/v2/send"))
            .and(body_json(serde_json::json!({
                "message": "hello signal",
                "number": "+15550000000",
                "recipients": ["+15550000001"]
            })))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        adapter
            .send_message("+15550000001", "hello signal")
            .await
            .expect("send should succeed");
    }

    #[tokio::test]
    async fn send_message_includes_recipient() {
        let server = MockServer::start().await;
        let adapter = SignalAdapter::new(server.uri(), "+15550000000".to_owned());
        Mock::given(method("POST"))
            .and(path("/v2/send"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        adapter
            .send_message("+15559999999", "hello")
            .await
            .expect("send should succeed");

        let requests = server
            .received_requests()
            .await
            .expect("requests should be available");
        let body: Value =
            serde_json::from_slice(&requests[0].body).expect("request body should parse");
        let recipients = body
            .get("recipients")
            .and_then(Value::as_array)
            .expect("recipients should be an array");
        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].as_str(), Some("+15559999999"));
    }

    #[tokio::test]
    async fn send_message_handles_api_error() {
        let server = MockServer::start().await;
        let adapter = SignalAdapter::new(server.uri(), "+15550000000".to_owned());
        Mock::given(method("POST"))
            .and(path("/v2/send"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let err = adapter
            .send_message("+15550000001", "hello")
            .await
            .expect_err("send should fail on api error");
        assert!(err.contains("500"));
    }

    #[tokio::test]
    async fn send_message_splits_long_text() {
        let server = MockServer::start().await;
        let adapter = SignalAdapter::new(server.uri(), "+15550000000".to_owned());
        Mock::given(method("POST"))
            .and(path("/v2/send"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let long_text = "a".repeat(5000);
        adapter
            .send_message("+15550000001", &long_text)
            .await
            .expect("send should succeed");

        let requests = server
            .received_requests()
            .await
            .expect("requests should be available");
        assert_eq!(requests.len(), 3, "expected 3 chunks for 5000 chars");
        for request in requests {
            let body: Value =
                serde_json::from_slice(&request.body).expect("request body should parse");
            let message = body
                .get("message")
                .and_then(Value::as_str)
                .expect("message should be string");
            assert!(message.len() <= 2000);
        }
    }
}
