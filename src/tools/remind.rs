use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Local};
use matrix_sdk::ruma::OwnedRoomId;
use tokio::time::sleep;

use crate::tools::Tool;

#[derive(Clone)]
pub struct ReminderStore {
    reminders: Arc<Mutex<Vec<Reminder>>>,
}

#[derive(Clone, Debug)]
struct Reminder {
    message: String,
    fire_at: DateTime<Local>,
    user_id: String,
    room_id: String,
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

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let message = params
            .get("message")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing required param: message".to_owned())?;
        let delay_minutes = params
            .get("delay_minutes")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| "missing required param: delay_minutes".to_owned())?;
        if delay_minutes < 0 {
            return Err("delay_minutes must be >= 0".to_owned());
        }

        let user_id = params
            .get("user_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown_user");
        let room_id = params
            .get("room_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown_room");

        let fire_at = Local::now() + Duration::minutes(delay_minutes);
        let reminder = Reminder {
            message: message.to_owned(),
            fire_at,
            user_id: user_id.to_owned(),
            room_id: room_id.to_owned(),
        };

        let mut guard = self
            .store
            .reminders
            .lock()
            .map_err(|_| "failed to acquire reminder lock".to_owned())?;
        guard.push(reminder);

        Ok(format!(
            "reminder set for {}",
            fire_at.format("%A, %B %-d, %Y at %-I:%M %p %Z")
        ))
    }
}

pub async fn run_reminder_loop(store: ReminderStore, client: matrix_sdk::Client) {
    loop {
        sleep(std::time::Duration::from_secs(30)).await;

        let now = Local::now();
        let due = {
            let mut guard = match store.reminders.lock() {
                Ok(guard) => guard,
                Err(err) => {
                    tracing::error!(error = %err, "reminder store lock poisoned");
                    continue;
                }
            };

            let mut due = Vec::new();
            let mut pending = Vec::new();
            for reminder in guard.drain(..) {
                if reminder.fire_at <= now {
                    due.push(reminder);
                } else {
                    pending.push(reminder);
                }
            }
            *guard = pending;
            due
        };

        for reminder in due {
            let room_id: Result<OwnedRoomId, _> = reminder.room_id.as_str().try_into();
            let Ok(room_id) = room_id else {
                tracing::warn!(room_id = %reminder.room_id, "invalid reminder room_id");
                continue;
            };

            let Some(room) = client.get_room(&room_id) else {
                tracing::warn!(room_id = %room_id, "room not found for reminder");
                continue;
            };

            if let Err(err) = room
                .send(
                    matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(
                        format!("🌿 reminder: {}", reminder.message),
                    ),
                )
                .await
            {
                tracing::error!(
                    error = %err,
                    room_id = %room_id,
                    user_id = %reminder.user_id,
                    "failed to send reminder"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Local};

    use super::{RemindTool, ReminderStore};
    use crate::tools::Tool;

    #[tokio::test]
    async fn set_reminder_stores() {
        let store = ReminderStore::new();
        let tool = RemindTool::new(store.clone());

        let result = tool
            .execute(serde_json::json!({
                "message": "stretch",
                "delay_minutes": 30,
                "user_id": "@jason:kcodes.me",
                "room_id": "!room:kcodes.me"
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
    async fn reminder_fire_time_correct() {
        let store = ReminderStore::new();
        let tool = RemindTool::new(store.clone());

        let before = Local::now();
        let _ = tool
            .execute(serde_json::json!({
                "message": "stretch",
                "delay_minutes": 30,
                "user_id": "@jason:kcodes.me",
                "room_id": "!room:kcodes.me"
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
}
