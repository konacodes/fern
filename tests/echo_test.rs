use fern::{format_echo, should_echo, EchoMessage};

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
    let own_id = "@fern:example.org";
    let sender = own_id;

    assert_eq!(
        should_echo(sender, own_id, EchoMessage::Text("hello")),
        None
    );
}

#[test]
fn echo_ignores_non_text() {
    let own_id = "@fern:example.org";
    let sender = "@alice:example.org";

    assert_eq!(should_echo(sender, own_id, EchoMessage::Image), None);
    assert_eq!(should_echo(sender, own_id, EchoMessage::Video), None);
    assert_eq!(should_echo(sender, own_id, EchoMessage::File), None);
}
