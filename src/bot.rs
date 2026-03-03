use std::{sync::Arc, time::Duration};

use futures_util::future::BoxFuture;
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

use crate::{
    db::messages::delete_room_messages,
    memory::{write_memory, MEMORY_TEMPLATE},
    orchestrator::engine::Orchestrator,
    sender::split_message,
    Config,
};

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
    pub orchestrator: Arc<Orchestrator>,
}

impl FernBot {
    pub async fn new(config: Config, orchestrator: Arc<Orchestrator>) -> BotResult<Self> {
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

        Ok(Self {
            client,
            orchestrator,
        })
    }

    pub async fn run(&self) -> BotResult<()> {
        let own_id = self
            .client
            .user_id()
            .ok_or("client has no logged in user id")?
            .to_owned();
        let message_own_id: OwnedUserId = own_id.clone();
        let invite_own_id: OwnedUserId = own_id;
        let orchestrator = Arc::clone(&self.orchestrator);

        self.client
            .add_event_handler(move |event: SyncRoomMessageEvent, room: Room| {
                let own_id = message_own_id.clone();
                let orchestrator = Arc::clone(&orchestrator);
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
                    tracing::info!(
                        room_id = %room.room_id(),
                        sender = %event.sender(),
                        message = %text,
                        "processing inbound text message"
                    );

                    let response = if text.trim() == "/reset" {
                        match write_memory(&orchestrator.data_dir, MEMORY_TEMPLATE) {
                            Ok(()) => match delete_room_messages(&orchestrator.db, room.room_id().as_ref()).await {
                                Ok(()) => "factory reset complete 🌿 fresh start".to_owned(),
                                Err(err) => {
                                    tracing::error!(error = %err, "failed to clear room messages during reset");
                                    "hmm i couldn't reset chat history right now".to_owned()
                                }
                            },
                            Err(err) => {
                                tracing::error!(error = %err, "failed to reset memory file");
                                "hmm i couldn't reset memory right now".to_owned()
                            }
                        }
                    } else {
                        let interim_room = room.clone();
                        let send_fn =
                            move |message: String| -> BoxFuture<'static, Result<(), String>> {
                            let interim_room = interim_room.clone();
                            Box::pin(async move {
                                interim_room
                                    .send(RoomMessageEventContent::text_plain(message))
                                    .await
                                    .map(|_| ())
                                    .map_err(|err| err.to_string())
                            })
                        };

                        match orchestrator
                            .process_message(
                                event.sender().as_ref(),
                                room.room_id().as_ref(),
                                text,
                                send_fn,
                            )
                            .await
                        {
                            Ok(response) => response,
                            Err(err) => {
                                tracing::error!(error = %err, "orchestrator processing failed");
                                "hmm something went wrong on my end, give me a sec 🌿".to_owned()
                            }
                        }
                    };
                    tracing::debug!(
                        room_id = %room.room_id(),
                        response = %response,
                        "orchestrator returned response text"
                    );

                    let chunks = split_message(&response, 500);
                    if chunks.is_empty() {
                        tracing::warn!(room_id = %room.room_id(), "response split into zero chunks");
                        return;
                    }
                    tracing::debug!(
                        room_id = %room.room_id(),
                        chunk_count = chunks.len(),
                        "sending response chunks"
                    );

                    let chunk_count = chunks.len();
                    for (idx, chunk) in chunks.into_iter().enumerate() {
                        tracing::trace!(
                            room_id = %room.room_id(),
                            chunk_index = idx,
                            chunk = %chunk,
                            "sending response chunk"
                        );
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
    use std::sync::{Arc, RwLock};

    use super::FernBot;
    use crate::{
        ai::cerebras::CerebrasClient, orchestrator::engine::Orchestrator, tools::ToolRegistry,
        Config,
    };

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
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-20250514".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        };

        let db = crate::db::init_db("sqlite::memory:")
            .await
            .expect("db should initialize");
        let cerebras = Arc::new(CerebrasClient::new(&config));
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let orchestrator = Arc::new(Orchestrator::new(
            cerebras,
            registry,
            config.data_dir.clone(),
            db,
        ));

        let result = FernBot::new(config, orchestrator).await;
        assert!(result.is_err(), "expected Err for invalid homeserver URL");
    }
}
