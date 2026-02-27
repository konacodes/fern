use std::{sync::Arc, time::Duration};

use matrix_sdk::{
    config::SyncSettings,
    room::Room,
    ruma::{
        events::room::{
            member::{MembershipState, StrippedRoomMemberEvent},
            message::{MessageType, RoomMessageEventContent, SyncRoomMessageEvent},
        },
        OwnedUserId, UserId,
    },
    Client,
};
use tokio::time::sleep;

use crate::{engine::conversation::ConversationEngine, sender::split_message, Config};

pub type BotResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EchoMessage<'a> {
    Text(&'a str),
    Image,
    Video,
    File,
    Other,
}

pub fn format_echo(text: &str) -> String {
    format!("🌿 {text}")
}

pub fn should_echo(sender: &UserId, own_id: &UserId, msg: EchoMessage<'_>) -> Option<String> {
    if sender == own_id {
        return None;
    }

    match msg {
        EchoMessage::Text(text) => Some(format_echo(text)),
        EchoMessage::Image | EchoMessage::Video | EchoMessage::File | EchoMessage::Other => None,
    }
}

pub struct FernBot {
    pub client: Client,
    pub engine: Arc<ConversationEngine>,
}

impl FernBot {
    pub async fn new(config: Config, engine: Arc<ConversationEngine>) -> BotResult<Self> {
        let client = Client::builder()
            .homeserver_url(config.homeserver_url.clone())
            .build()
            .await?;

        client
            .matrix_auth()
            .login_username(&config.bot_user, &config.bot_password)
            .send()
            .await?;

        tracing::info!(user = %config.bot_user, "logged in");

        Ok(Self { client, engine })
    }

    pub async fn run(&self) -> BotResult<()> {
        let own_id = self
            .client
            .user_id()
            .ok_or("client has no logged in user id")?
            .to_owned();
        let message_own_id: OwnedUserId = own_id.clone();
        let invite_own_id: OwnedUserId = own_id;
        let engine = Arc::clone(&self.engine);

        self.client
            .add_event_handler(move |event: SyncRoomMessageEvent, room: Room| {
                let own_id = message_own_id.clone();
                let engine = Arc::clone(&engine);
                async move {
                    tracing::debug!(
                        room_id = %room.room_id(),
                        sender = %event.sender(),
                        "received room message event"
                    );

                    let Some(original) = event.as_original() else {
                        return;
                    };

                    if event.sender() == own_id {
                        return;
                    }

                    let text = match &original.content.msgtype {
                        MessageType::Text(text) => text.body.as_str(),
                        _ => return,
                    };

                    let response = match engine
                        .respond(event.sender().as_ref(), room.room_id().as_ref(), text)
                        .await
                    {
                        Ok(response) => response,
                        Err(err) => {
                            tracing::error!(error = %err, "conversation engine failed");
                            return;
                        }
                    };

                    let chunks = split_message(&response, 500);
                    if chunks.is_empty() {
                        return;
                    }

                    let chunk_count = chunks.len();
                    for (idx, chunk) in chunks.into_iter().enumerate() {
                        if let Err(err) =
                            room.send(RoomMessageEventContent::text_plain(chunk)).await
                        {
                            tracing::error!(error = %err, "failed to send bot response chunk");
                            break;
                        }

                        if idx + 1 < chunk_count {
                            sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
            });

        self.client
            .add_event_handler(move |event: StrippedRoomMemberEvent, room: Room| {
                let own_id = invite_own_id.clone();
                async move {
                    if event.state_key != own_id {
                        return;
                    }
                    if event.content.membership != MembershipState::Invite {
                        return;
                    }

                    match room.join().await {
                        Ok(()) => {
                            tracing::info!(room_id = %room.room_id(), "accepted room invite");
                        }
                        Err(err) => {
                            tracing::error!(error = %err, room_id = %room.room_id(), "failed to accept invite");
                        }
                    }
                }
            });

        self.client.sync(SyncSettings::default()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::FernBot;
    use crate::Config;

    #[tokio::test]
    async fn fern_bot_new_returns_err_for_bad_homeserver_url() {
        let temp_dir = std::env::temp_dir().join(format!("fern-bad-url-{}", std::process::id()));
        let config = Config {
            homeserver_url: "not-a-url".to_owned(),
            bot_user: "@fern:local".to_owned(),
            bot_password: "secret".to_owned(),
            data_dir: temp_dir.to_string_lossy().to_string(),
            cerebras_api_key: "test-key".to_owned(),
            cerebras_model: "qwen-3-235b".to_owned(),
            cerebras_base_url: "https://api.cerebras.ai/v1".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        };

        let db = crate::db::init_db("sqlite::memory:")
            .await
            .expect("db should initialize");
        let cerebras = crate::ai::cerebras::CerebrasClient::new(&config);
        let engine = Arc::new(crate::engine::conversation::ConversationEngine::new(
            cerebras, db,
        ));

        let result = FernBot::new(config, engine).await;
        assert!(result.is_err(), "expected Err for invalid homeserver URL");
    }
}
