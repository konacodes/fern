use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

use crate::tools::{
    dynamic::{DynamicToolDef, DynamicToolType},
    Tool,
};

pub struct HttpTool {
    def: DynamicToolDef,
    http: reqwest::Client,
}

impl HttpTool {
    pub fn new(def: DynamicToolDef) -> Result<Self, String> {
        match def.tool_type {
            DynamicToolType::Http { .. } => Ok(Self {
                def,
                http: reqwest::Client::new(),
            }),
            _ => Err("http tool requires Http dynamic tool type".to_owned()),
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn parameters(&self) -> &str {
        "dynamic http parameters"
    }

    fn tool_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for param in &self.def.parameters {
            properties.insert(
                param.name.clone(),
                json!({
                    "type": param.param_type,
                    "description": param.description,
                }),
            );
            if param.required {
                required.push(param.name.clone());
            }
        }

        json!({
            "type": "function",
            "function": {
                "name": self.def.name.clone(),
                "strict": true,
                "description": self.def.description.clone(),
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let param_map = params
            .as_object()
            .ok_or_else(|| "http tool params must be a JSON object".to_owned())?;

        for param in &self.def.parameters {
            if param.required && !param_map.contains_key(&param.name) {
                return Err(format!("missing required param: {}", param.name));
            }
        }

        let (url_template, method_name, headers, body_template, response_jq) =
            match &self.def.tool_type {
                DynamicToolType::Http {
                    url_template,
                    method,
                    headers,
                    body_template,
                    response_jq,
                } => (url_template, method, headers, body_template, response_jq),
                DynamicToolType::Script { .. } => {
                    return Err("http tool requires Http dynamic tool type".to_owned());
                }
            };

        let url = render_template(url_template, param_map)?;
        let method = reqwest::Method::from_bytes(method_name.as_bytes())
            .map_err(|err| format!("invalid http method {method_name}: {err}"))?;

        let mut request = self
            .http
            .request(method, &url)
            .timeout(Duration::from_secs(10));
        for (header, value) in headers {
            request = request.header(header, value);
        }
        if let Some(template) = body_template {
            let body = render_template(template, param_map)?;
            request = request.body(body);
        }

        let response = request.send().await.map_err(|err| {
            if err.is_timeout() {
                "http request timeout after 10 seconds".to_owned()
            } else {
                format!("http request failed: {err}")
            }
        })?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| format!("failed reading HTTP response body: {err}"))?;
        if !status.is_success() {
            return Err(format!(
                "http request returned {}: {}",
                status.as_u16(),
                truncate_response(&body)
            ));
        }

        if let Some(path) = response_jq {
            let value = serde_json::from_str::<serde_json::Value>(&body)
                .map_err(|err| format!("response_jq requires JSON response: {err}"))?;
            return extract_json_path(&value, path)
                .ok_or_else(|| format!("response_jq path not found: {path}"));
        }

        Ok(truncate_response(&body))
    }
}

pub fn extract_json_path(value: &serde_json::Value, path: &str) -> Option<String> {
    if !path.starts_with('.') {
        return None;
    }
    let mut current = value;
    let tokens = path.trim_start_matches('.').split('.');

    for token in tokens {
        if token.is_empty() {
            return None;
        }
        let (key, indices) = parse_path_token(token)?;

        if !key.is_empty() {
            current = current.get(&key)?;
        }
        for idx in indices {
            current = current.get(idx)?;
        }
    }

    match current {
        serde_json::Value::String(text) => Some(text.clone()),
        _ => Some(current.to_string()),
    }
}

fn parse_path_token(token: &str) -> Option<(String, Vec<usize>)> {
    let mut key = String::new();
    let mut indices = Vec::new();
    let mut chars = token.chars().peekable();

    while let Some(ch) = chars.peek() {
        if *ch == '[' {
            break;
        }
        key.push(*ch);
        chars.next();
    }

    while let Some(ch) = chars.next() {
        if ch != '[' {
            return None;
        }
        let mut idx_text = String::new();
        for next in chars.by_ref() {
            if next == ']' {
                break;
            }
            idx_text.push(next);
        }
        if idx_text.is_empty() || !idx_text.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        indices.push(idx_text.parse::<usize>().ok()?);
    }

    Some((key, indices))
}

fn render_template(
    template: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<String, String> {
    let mut rendered = template.to_owned();
    for (name, value) in params {
        let raw = value_to_string(value)?;
        let encoded = percent_encode(&raw);
        rendered = rendered.replace(&format!("{{{{{name}}}}}"), &encoded);
    }
    Ok(replace_unbound_placeholders(&rendered))
}

fn value_to_string(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::String(text) => Ok(text.clone()),
        serde_json::Value::Number(number) => Ok(number.to_string()),
        serde_json::Value::Bool(boolean) => Ok(boolean.to_string()),
        _ => Err("template params must be string/number/boolean".to_owned()),
    }
}

fn percent_encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        let safe = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if safe {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[(byte >> 4) as usize]));
            out.push(char::from(HEX[(byte & 0x0F) as usize]));
        }
    }
    out
}

fn truncate_response(body: &str) -> String {
    body.chars().take(2000).collect::<String>()
}

fn replace_unbound_placeholders(template: &str) -> String {
    let mut output = String::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("}}") {
            let placeholder = after_start[..end].trim();
            tracing::debug!(
                placeholder,
                "replacing unbound template placeholder with empty string"
            );
            rest = &after_start[end + 2..];
        } else {
            output.push_str(&rest[start..]);
            return output;
        }
    }

    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use serde_json::json;
    use wiremock::{
        matchers::{body_string, method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::tools::{
        dynamic::{DynamicToolDef, DynamicToolType, ToolParam},
        Tool,
    };

    use super::{extract_json_path, HttpTool};

    fn weather_tool(url_template: String) -> DynamicToolDef {
        DynamicToolDef {
            name: "weather_lookup".to_owned(),
            description: "fetch weather for a city".to_owned(),
            parameters: vec![ToolParam {
                name: "location".to_owned(),
                param_type: "string".to_owned(),
                description: "city name".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Http {
                url_template,
                method: "GET".to_owned(),
                headers: HashMap::new(),
                body_template: None,
                response_jq: None,
            },
        }
    }

    #[tokio::test]
    async fn http_tool_get_request() {
        let server = MockServer::start().await;
        let tool = HttpTool::new(weather_tool(format!(
            "{}/weather?q={{{{location}}}}",
            server.uri()
        )))
        .expect("tool should build");

        Mock::given(method("GET"))
            .and(path("/weather"))
            .and(query_param("q", "Austin"))
            .respond_with(ResponseTemplate::new(200).set_body_string("sunny"))
            .expect(1)
            .mount(&server)
            .await;

        let result = tool
            .execute(json!({ "location": "Austin" }))
            .await
            .expect("tool should succeed");
        assert_eq!(result, "sunny");
    }

    #[tokio::test]
    async fn http_tool_post_with_body() {
        let server = MockServer::start().await;
        let def = DynamicToolDef {
            name: "post_weather".to_owned(),
            description: "post weather".to_owned(),
            parameters: vec![ToolParam {
                name: "location".to_owned(),
                param_type: "string".to_owned(),
                description: "city".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Http {
                url_template: format!("{}/echo", server.uri()),
                method: "POST".to_owned(),
                headers: HashMap::new(),
                body_template: Some("{\"city\":\"{{location}}\"}".to_owned()),
                response_jq: None,
            },
        };
        let tool = HttpTool::new(def).expect("tool should build");

        Mock::given(method("POST"))
            .and(path("/echo"))
            .and(body_string("{\"city\":\"Austin\"}"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;

        let result = tool
            .execute(json!({ "location": "Austin" }))
            .await
            .expect("tool should succeed");
        assert_eq!(result, "ok");
    }

    #[tokio::test]
    async fn http_tool_url_encoding() {
        let server = MockServer::start().await;
        let tool = HttpTool::new(weather_tool(format!(
            "{}/weather?q={{{{location}}}}",
            server.uri()
        )))
        .expect("tool should build");

        Mock::given(method("GET"))
            .and(path("/weather"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;

        let _ = tool
            .execute(json!({ "location": "new york/tx" }))
            .await
            .expect("tool should succeed");
        let requests = server
            .received_requests()
            .await
            .expect("requests should be retrievable");
        let url = requests[0].url.as_str();
        assert!(
            url.contains("q=new%20york%2Ftx"),
            "expected URL-encoded param, got: {url}"
        );
    }

    #[tokio::test]
    async fn http_tool_response_jq() {
        let server = MockServer::start().await;
        let def = DynamicToolDef {
            name: "extract".to_owned(),
            description: "extract value".to_owned(),
            parameters: vec![],
            tool_type: DynamicToolType::Http {
                url_template: format!("{}/data", server.uri()),
                method: "GET".to_owned(),
                headers: HashMap::new(),
                body_template: None,
                response_jq: Some(".data.value".to_owned()),
            },
        };
        let tool = HttpTool::new(def).expect("tool should build");

        Mock::given(method("GET"))
            .and(path("/data"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "value": "42"
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = tool.execute(json!({})).await.expect("tool should succeed");
        assert_eq!(result, "42");
    }

    #[tokio::test]
    async fn http_tool_timeout() {
        let server = MockServer::start().await;
        let tool = HttpTool::new(weather_tool(format!(
            "{}/slow?q={{{{location}}}}",
            server.uri()
        )))
        .expect("tool should build");

        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(11))
                    .set_body_string("too slow"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = tool
            .execute(json!({ "location": "austin" }))
            .await
            .expect_err("tool should timeout");
        assert!(
            err.to_lowercase().contains("timeout"),
            "expected timeout error, got: {err}"
        );
    }

    #[tokio::test]
    async fn http_tool_truncates_long_response() {
        let server = MockServer::start().await;
        let tool = HttpTool::new(weather_tool(format!(
            "{}/long?q={{{{location}}}}",
            server.uri()
        )))
        .expect("tool should build");

        Mock::given(method("GET"))
            .and(path("/long"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(2500)))
            .expect(1)
            .mount(&server)
            .await;

        let result = tool
            .execute(json!({ "location": "austin" }))
            .await
            .expect("tool should succeed");
        assert_eq!(result.len(), 2000);
    }

    #[tokio::test]
    async fn http_tool_allows_missing_optional_template_param() {
        let server = MockServer::start().await;
        let def = DynamicToolDef {
            name: "optional_query".to_owned(),
            description: "supports optional category".to_owned(),
            parameters: vec![ToolParam {
                name: "country".to_owned(),
                param_type: "string".to_owned(),
                description: "country code".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Http {
                url_template: format!(
                    "{}/news?country={{{{country}}}}&category={{{{category}}}}",
                    server.uri()
                ),
                method: "GET".to_owned(),
                headers: HashMap::new(),
                body_template: None,
                response_jq: None,
            },
        };
        let tool = HttpTool::new(def).expect("tool should build");

        Mock::given(method("GET"))
            .and(path("/news"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;

        let result = tool
            .execute(json!({ "country": "us" }))
            .await
            .expect("tool should succeed");
        assert_eq!(result, "ok");

        let requests = server
            .received_requests()
            .await
            .expect("requests should be retrievable");
        let url = requests[0].url.as_str();
        assert!(
            url.contains("country=us"),
            "expected country query param in url, got: {url}"
        );
        assert!(
            url.contains("category="),
            "expected optional category placeholder to resolve as empty, got: {url}"
        );
    }

    #[test]
    fn extract_json_path_basic() {
        let value = json!({
            "foo": {
                "bar": {
                    "baz": "ok"
                }
            }
        });
        let extracted = extract_json_path(&value, ".foo.bar.baz");
        assert_eq!(extracted.as_deref(), Some("ok"));
    }

    #[test]
    fn extract_json_path_array() {
        let value = json!({
            "items": [
                { "name": "a" },
                { "name": "b" }
            ]
        });
        let extracted = extract_json_path(&value, ".items[1].name");
        assert_eq!(extracted.as_deref(), Some("b"));
    }

    #[test]
    fn extract_json_path_missing() {
        let value = json!({ "foo": "bar" });
        assert!(extract_json_path(&value, ".foo.bar").is_none());
        assert!(extract_json_path(&value, "foo").is_none());
    }
}
