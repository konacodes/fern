use matrix_sdk::{
    config::SyncSettings,
    room::Room,
    ruma::{
        events::room::message::{MessageType, RoomMessageEventContent, SyncRoomMessageEvent},
        OwnedUserId, UserId,
    },
    Client,
};

use crate::Config;

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
    pub config: Config,
}

impl FernBot {
    pub async fn new(config: Config) -> BotResult<Self> {
        std::fs::create_dir_all(&config.data_dir)?;

        let client = Client::builder()
            .homeserver_url(config.homeserver_url.clone())
            .sqlite_store(&config.data_dir, None)
            .build()
            .await?;

        client
            .matrix_auth()
            .login_username(&config.bot_user, &config.bot_password)
            .send()
            .await?;

        tracing::info!(user = %config.bot_user, "logged in");

        Ok(Self { client, config })
    }

    pub async fn run(&self) -> BotResult<()> {
        let own_id: OwnedUserId = self.config.bot_user.as_str().try_into()?;
        self.client
            .add_event_handler(move |event: SyncRoomMessageEvent, room: Room| {
                let own_id = own_id.clone();
                async move {
                    tracing::debug!(
                        room_id = %room.room_id(),
                        sender = %event.sender(),
                        "received room message event"
                    );

                    let Some(original) = event.as_original() else {
                        return;
                    };

                    let message = match &original.content.msgtype {
                        MessageType::Text(text) => EchoMessage::Text(&text.body),
                        MessageType::Image(_) => EchoMessage::Image,
                        MessageType::Video(_) => EchoMessage::Video,
                        MessageType::File(_) => EchoMessage::File,
                        _ => EchoMessage::Other,
                    };

                    let Some(response) = should_echo(event.sender(), own_id.as_ref(), message)
                    else {
                        return;
                    };

                    if let Err(err) = room
                        .send(RoomMessageEventContent::text_plain(response))
                        .await
                    {
                        tracing::error!(error = %err, "failed to send echo response");
                    }
                }
            });

        self.client.sync(SyncSettings::default()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        };

        let result = FernBot::new(config).await;
        assert!(result.is_err(), "expected Err for invalid homeserver URL");
    }
}
