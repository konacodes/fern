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

pub fn should_echo(sender: &str, own_id: &str, msg: EchoMessage<'_>) -> Option<String> {
    if sender == own_id {
        return None;
    }

    match msg {
        EchoMessage::Text(text) => Some(format_echo(text)),
        EchoMessage::Image | EchoMessage::Video | EchoMessage::File | EchoMessage::Other => None,
    }
}
