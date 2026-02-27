use fern::{format_echo, should_echo, EchoMessage};
use matrix_sdk::ruma::user_id;

#[test]
fn echo_format() {
    assert_eq!(format_echo("hello"), "🌿 hello");
}

#[test]
fn echo_format_empty() {
    assert_eq!(format_echo(""), "🌿 ");
}

#[test]
fn echo_format_unicode() {
    assert_eq!(format_echo("こんにちは"), "🌿 こんにちは");
}

#[test]
fn echo_ignores_own_messages() {
    let own_id = user_id!("@fern:example.org").to_owned();
    let sender = own_id.clone();

    assert_eq!(
        should_echo(&sender, &own_id, EchoMessage::Text("hello")),
        None
    );
}

#[test]
fn echo_ignores_non_text() {
    let own_id = user_id!("@fern:example.org").to_owned();
    let sender = user_id!("@alice:example.org").to_owned();

    assert_eq!(should_echo(&sender, &own_id, EchoMessage::Image), None);
    assert_eq!(should_echo(&sender, &own_id, EchoMessage::Video), None);
    assert_eq!(should_echo(&sender, &own_id, EchoMessage::File), None);
}
