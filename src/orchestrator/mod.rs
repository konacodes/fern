pub mod engine;

pub const ORCHESTRATOR_PROMPT: &str = r#"you are fern's brain. you receive a message and decide what to do.

you can either:
1. respond directly with text (for simple conversation)
2. call a tool to get information or do something, then respond

when you want to call a tool, respond with EXACTLY this format:
<tool_call>
{"tool": "tool_name", "params": {"key": "value"}}
</tool_call>

when you want to respond with text, just write your response normally — no special tags.

rules:
- you can call ONE tool per response. after getting the result, you can call another or respond.
- don't explain that you're calling a tool to the user. just do it.
- if a tool fails, handle it gracefully and tell the user what happened.
- don't make up tools that don't exist. only use what's available.
- stay in character as fern (lowercase, casual, brief)."#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrchestratorAction {
    Respond(String),
    CallTool {
        tool_name: String,
        params: serde_json::Value,
        interim_text: Option<String>,
    },
}

#[derive(serde::Deserialize)]
struct ToolCallPayload {
    tool: String,
    params: serde_json::Value,
}

pub fn parse_response(raw: &str) -> OrchestratorAction {
    let open_tag = "<tool_call>";
    let close_tag = "</tool_call>";

    let Some(start) = raw.find(open_tag) else {
        return OrchestratorAction::Respond(raw.to_owned());
    };
    let json_start = start + open_tag.len();

    let Some(end_relative) = raw[json_start..].find(close_tag) else {
        return OrchestratorAction::Respond(raw.to_owned());
    };
    let json_end = json_start + end_relative;
    let json_str = raw[json_start..json_end].trim();

    let Ok(payload) = serde_json::from_str::<ToolCallPayload>(json_str) else {
        return OrchestratorAction::Respond(raw.to_owned());
    };

    let interim_text = raw[..start].trim();
    OrchestratorAction::CallTool {
        tool_name: payload.tool,
        params: payload.params,
        interim_text: if interim_text.is_empty() {
            None
        } else {
            Some(interim_text.to_owned())
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_response, OrchestratorAction};

    #[test]
    fn parse_plain_text() {
        let action = parse_response("hey what's up");
        assert_eq!(
            action,
            OrchestratorAction::Respond("hey what's up".to_owned())
        );
    }

    #[test]
    fn parse_tool_call() {
        let raw = r#"<tool_call>{"tool":"memory_read","params":{}}</tool_call>"#;
        let action = parse_response(raw);

        assert_eq!(
            action,
            OrchestratorAction::CallTool {
                tool_name: "memory_read".to_owned(),
                params: json!({}),
                interim_text: None,
            }
        );
    }

    #[test]
    fn parse_tool_call_with_interim() {
        let raw = "let me check on that\n<tool_call>{\"tool\":\"current_time\",\"params\":{}}</tool_call>";
        let action = parse_response(raw);

        assert_eq!(
            action,
            OrchestratorAction::CallTool {
                tool_name: "current_time".to_owned(),
                params: json!({}),
                interim_text: Some("let me check on that".to_owned()),
            }
        );
    }

    #[test]
    fn parse_malformed_tool_call() {
        let raw = "<tool_call>not json</tool_call>";
        let action = parse_response(raw);
        assert_eq!(action, OrchestratorAction::Respond(raw.to_owned()));
    }

    #[test]
    fn parse_tool_call_with_params() {
        let raw = r#"<tool_call>{"tool":"remind","params":{"message":"call mom","delay_minutes":30}}</tool_call>"#;
        let action = parse_response(raw);

        assert_eq!(
            action,
            OrchestratorAction::CallTool {
                tool_name: "remind".to_owned(),
                params: json!({"message":"call mom","delay_minutes":30}),
                interim_text: None,
            }
        );
    }
}
