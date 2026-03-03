use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Local};
use serde_json::json;
use tokio::time::sleep;

use crate::{
    adapter::MessagingAdapter,
    ai::cerebras::{CerebrasClient, ChatMessage},
    tools::Tool,
};

#[derive(Clone)]
pub struct ReminderStore {
    reminders: Arc<Mutex<Vec<Reminder>>>,
}

#[derive(Clone, Debug)]
struct Reminder {
    message: String,
    fire_at: DateTime<Local>,
    user_id: String,
    conversation_id: String,
}

impl ReminderStore {
    pub fn new() -> Self {
        Self {
            reminders: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for ReminderStore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RemindTool {
    store: ReminderStore,
}

impl RemindTool {
    pub fn new(store: ReminderStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for RemindTool {
    fn name(&self) -> &str {
        "set_reminder"
    }

    fn description(&self) -> &str {
        "set a reminder that will be sent to the user after a delay. good for 'remind me in 30 minutes to...' type requests"
    }

    fn parameters(&self) -> &str {
        "message (string): what to remind about, delay_minutes (integer): how many minutes from now"
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
                        "message": {
                            "type": "string",
                            "description": "what to remind about"
                        },
                        "delay_minutes": {
                            "type": "integer",
                            "description": "how many minutes from now"
                        }
                    },
                    "required": ["message", "delay_minutes"],
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        tracing::debug!(
            params = %params,
            "set_reminder invoked"
        );
        let message = params
            .get("message")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing required param: message".to_owned())?;
        let delay_value = params
            .get("delay_minutes")
            .ok_or_else(|| "missing required param: delay_minutes".to_owned())?;
        let delay_minutes = if let Some(minutes) = delay_value.as_i64() {
            minutes
        } else if let Some(minutes) = delay_value.as_str() {
            minutes
                .parse::<i64>()
                .map_err(|_| "delay_minutes must be an integer".to_owned())?
        } else {
            return Err("delay_minutes must be an integer".to_owned());
        };
        if delay_minutes < 0 {
            return Err("delay_minutes must be >= 0".to_owned());
        }

        let user_id = params
            .get("user_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing required context: user_id".to_owned())?;
        let conversation_id = params
            .get("conversation_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing required context: conversation_id".to_owned())?;

        let fire_at = Local::now() + Duration::minutes(delay_minutes);
        let reminder = Reminder {
            message: message.to_owned(),
            fire_at,
            user_id: user_id.to_owned(),
            conversation_id: conversation_id.to_owned(),
        };

        let mut guard = self
            .store
            .reminders
            .lock()
            .map_err(|_| "failed to acquire reminder lock".to_owned())?;
        guard.push(reminder);
        tracing::info!(
            user_id = %user_id,
            conversation_id = %conversation_id,
            fire_at = %fire_at.to_rfc3339(),
            "reminder scheduled"
        );

        Ok(format!(
            "reminder set for {}",
            fire_at.format("%A, %B %-d, %Y at %-I:%M %p %Z")
        ))
    }
}

fn partition_due_reminders(
    reminders: Vec<Reminder>,
    now: DateTime<Local>,
) -> (Vec<Reminder>, Vec<Reminder>) {
    let mut due = Vec::new();
    let mut pending = Vec::new();
    for reminder in reminders {
        if reminder.fire_at <= now {
            due.push(reminder);
        } else {
            pending.push(reminder);
        }
    }
    (due, pending)
}

fn with_retry_delay(mut reminder: Reminder, now: DateTime<Local>) -> Reminder {
    reminder.fire_at = now + Duration::seconds(30);
    reminder
}

async fn render_reminder_message(cerebras: &CerebrasClient, reminder_text: &str) -> String {
    const REMINDER_SYSTEM_PROMPT: &str = "you are fern, a friendly assistant. \
send a short reminder message. keep it to one sentence, lowercase, and concise.";
    let user_prompt = format!("send this reminder now: {reminder_text}");
    let fallback = format!("🌿 reminder: {reminder_text}");
    tracing::debug!(
        reminder_text = %reminder_text,
        "rendering reminder message with model"
    );

    let response = match cerebras
        .chat(
            REMINDER_SYSTEM_PROMPT,
            vec![ChatMessage::new("user", user_prompt)],
            None,
        )
        .await
    {
        Ok(response) => response,
        Err(err) => {
            tracing::warn!(error = %err, "failed to generate reminder text with model");
            return fallback;
        }
    };

    response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .map(|content| content.trim().to_owned())
        .filter(|content| !content.is_empty())
        .unwrap_or(fallback)
}

pub async fn run_reminder_loop(
    store: ReminderStore,
    adapter: Arc<dyn MessagingAdapter>,
    cerebras: Arc<CerebrasClient>,
) {
    tracing::info!("reminder loop started");
    loop {
        sleep(std::time::Duration::from_secs(30)).await;
        process_due_reminders_once(&store, Arc::clone(&adapter), Arc::clone(&cerebras)).await;
    }
}

pub async fn process_due_reminders_once(
    store: &ReminderStore,
    adapter: Arc<dyn MessagingAdapter>,
    cerebras: Arc<CerebrasClient>,
) {
    let now = Local::now();
    let (due, pending_count_after_partition) = {
        let mut guard = match store.reminders.lock() {
            Ok(guard) => guard,
            Err(err) => {
                tracing::error!(error = %err, "reminder store lock poisoned");
                return;
            }
        };
        let before_count = guard.len();
        let reminders = guard.drain(..).collect::<Vec<_>>();
        let (due, pending) = partition_due_reminders(reminders, now);
        let due_count = due.len();
        let pending_count = pending.len();
        *guard = pending;
        tracing::trace!(
            before_count,
            due_count,
            pending_count,
            now = %now.to_rfc3339(),
            "reminder loop partitioned reminders"
        );
        (due, pending_count)
    };

    let mut retries = Vec::new();
    for reminder in due {
        tracing::debug!(
            conversation_id = %reminder.conversation_id,
            user_id = %reminder.user_id,
            fire_at = %reminder.fire_at.to_rfc3339(),
            message = %reminder.message,
            "processing due reminder"
        );

        let reminder_text = render_reminder_message(cerebras.as_ref(), &reminder.message).await;
        if let Err(err) = adapter
            .send_message(&reminder.conversation_id, &reminder_text)
            .await
        {
            tracing::error!(
                error = %err,
                conversation_id = %reminder.conversation_id,
                user_id = %reminder.user_id,
                "failed to send reminder"
            );
            retries.push(with_retry_delay(reminder, now));
        } else {
            tracing::info!(
                conversation_id = %reminder.conversation_id,
                user_id = %reminder.user_id,
                "sent reminder"
            );
        }
    }

    if !retries.is_empty() {
        let mut guard = match store.reminders.lock() {
            Ok(guard) => guard,
            Err(err) => {
                tracing::error!(error = %err, "reminder store lock poisoned while retrying");
                return;
            }
        };
        let retry_count = retries.len();
        guard.extend(retries);
        tracing::warn!(
            retry_count,
            pending_count = guard.len(),
            pending_count_after_partition,
            "re-queued reminders after delivery failure"
        );
    } else {
        tracing::trace!(
            pending_count_after_partition,
            "reminder loop iteration completed with no retries"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, Local};
    use serde_json::json;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use super::{
        partition_due_reminders, process_due_reminders_once, with_retry_delay, RemindTool,
        Reminder, ReminderStore,
    };
    use crate::{
        adapter::{MessageHandler, MessagingAdapter},
        ai::cerebras::CerebrasClient,
        config::Config,
        tools::Tool,
    };

    struct MockAdapter {
        sent: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    }

    #[async_trait::async_trait]
    impl MessagingAdapter for MockAdapter {
        async fn run(&self, _handler: Arc<dyn MessageHandler>) -> Result<(), String> {
            Ok(())
        }

        async fn send_message(&self, conversation_id: &str, text: &str) -> Result<(), String> {
            self.sent
                .lock()
                .map_err(|_| "lock poisoned".to_owned())?
                .push((conversation_id.to_owned(), text.to_owned()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn set_reminder_stores() {
        let store = ReminderStore::new();
        let tool = RemindTool::new(store.clone());

        let result = tool
            .execute(serde_json::json!({
                "message": "stretch",
                "delay_minutes": 30,
                "user_id": "@jason:kcodes.me",
                "conversation_id": "+15550000001"
            }))
            .await
            .expect("execute should succeed");

        assert!(result.contains("reminder set"));
        let reminders = store
            .reminders
            .lock()
            .expect("reminder lock should not be poisoned");
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].message, "stretch");
    }

    #[tokio::test]
    async fn set_reminder_bad_params() {
        let store = ReminderStore::new();
        let tool = RemindTool::new(store);

        let err = tool
            .execute(serde_json::json!({
                "message": "stretch"
            }))
            .await
            .expect_err("execute should fail for missing delay");
        assert!(err.contains("delay_minutes"));
    }

    #[tokio::test]
    async fn set_reminder_accepts_string_delay() {
        let store = ReminderStore::new();
        let tool = RemindTool::new(store.clone());

        let result = tool
            .execute(serde_json::json!({
                "message": "stretch",
                "delay_minutes": "1",
                "user_id": "@jason:kcodes.me",
                "conversation_id": "+15550000001"
            }))
            .await
            .expect("execute should succeed");

        assert!(result.contains("reminder set"));
        let reminders = store
            .reminders
            .lock()
            .expect("reminder lock should not be poisoned");
        assert_eq!(reminders.len(), 1);
    }

    #[tokio::test]
    async fn set_reminder_missing_context_fails() {
        let store = ReminderStore::new();
        let tool = RemindTool::new(store);

        let err = tool
            .execute(serde_json::json!({
                "message": "stretch",
                "delay_minutes": 1
            }))
            .await
            .expect_err("execute should fail for missing room/user context");
        assert!(err.contains("context"));
    }

    #[tokio::test]
    async fn reminder_fire_time_correct() {
        let store = ReminderStore::new();
        let tool = RemindTool::new(store.clone());

        let before = Local::now();
        let _ = tool
            .execute(serde_json::json!({
                "message": "stretch",
                "delay_minutes": 30,
                "user_id": "@jason:kcodes.me",
                "conversation_id": "+15550000001"
            }))
            .await
            .expect("execute should succeed");
        let after = Local::now();

        let reminders = store
            .reminders
            .lock()
            .expect("reminder lock should not be poisoned");
        let fire_at = reminders[0].fire_at;
        let min = before + Duration::minutes(29);
        let max = after + Duration::minutes(31);
        assert!(
            fire_at >= min && fire_at <= max,
            "fire_at out of range: {fire_at}, expected between {min} and {max}"
        );
    }

    #[test]
    fn partition_due_separates_due_and_pending() {
        let now = Local::now();
        let due_reminder = Reminder {
            message: "due".to_owned(),
            fire_at: now - Duration::minutes(1),
            user_id: "@u:a".to_owned(),
            conversation_id: "+15550000001".to_owned(),
        };
        let pending_reminder = Reminder {
            message: "pending".to_owned(),
            fire_at: now + Duration::minutes(1),
            user_id: "@u:a".to_owned(),
            conversation_id: "+15550000002".to_owned(),
        };

        let (due, pending) = partition_due_reminders(vec![due_reminder, pending_reminder], now);
        assert_eq!(due.len(), 1);
        assert_eq!(pending.len(), 1);
        assert_eq!(due[0].message, "due");
        assert_eq!(pending[0].message, "pending");
    }

    #[test]
    fn with_retry_delay_pushes_fire_time_forward() {
        let now = Local::now();
        let reminder = Reminder {
            message: "retry me".to_owned(),
            fire_at: now - Duration::minutes(1),
            user_id: "@u:a".to_owned(),
            conversation_id: "+15550000001".to_owned(),
        };

        let retried = with_retry_delay(reminder, now);
        assert!(retried.fire_at > now);
    }

    #[tokio::test]
    async fn reminder_fires_via_adapter() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "🌿 reminder: stretch" } }]
            })))
            .mount(&server)
            .await;

        let store = ReminderStore::new();
        let tool = RemindTool::new(store.clone());
        tool.execute(json!({
            "message": "stretch",
            "delay_minutes": 0,
            "user_id": "@jason:kcodes.me",
            "conversation_id": "+15550000001"
        }))
        .await
        .expect("set reminder should succeed");

        let adapter_state = Arc::new(std::sync::Mutex::new(Vec::new()));
        let adapter: Arc<dyn MessagingAdapter> = Arc::new(MockAdapter {
            sent: Arc::clone(&adapter_state),
        });
        let config = Config {
            signal_api_url: "http://signal-api:8080".to_owned(),
            signal_account_number: "+15550000000".to_owned(),
            data_dir: "./data".to_owned(),
            cerebras_api_key: "test-key".to_owned(),
            cerebras_model: "qwen-3-235b".to_owned(),
            cerebras_base_url: format!("{}/v1", server.uri()),
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-20250514".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        };
        let cerebras = Arc::new(CerebrasClient::new(&config));

        process_due_reminders_once(&store, adapter, cerebras).await;

        let sent = adapter_state
            .lock()
            .expect("adapter sent lock should not be poisoned");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "+15550000001");
    }
}
