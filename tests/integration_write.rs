mod common;

use common::{spawn_relay, WsTestClient};
use moar::config::{EventPolicy, PolicyConfig, WritePolicy};
use nostr::{EventBuilder, Keys, Kind};

fn make_event(keys: &Keys, content: &str) -> nostr::Event {
    EventBuilder::text_note(content, [])
        .to_event(keys)
        .unwrap()
}

fn make_event_kind(keys: &Keys, kind: u16, content: &str) -> nostr::Event {
    EventBuilder::new(Kind::from(kind), content, [])
        .to_event(keys)
        .unwrap()
}

#[tokio::test]
async fn open_relay_accepts_event() {
    let (port, _store) = spawn_relay(PolicyConfig::default()).await;
    let mut client = WsTestClient::connect(port).await;

    let keys = Keys::generate();
    let event = make_event(&keys, "hello world");
    client.send_event(&event).await;

    let (status, _msg) = client.expect_ok().await;
    assert!(status, "open relay should accept event");
}

#[tokio::test]
async fn allow_list_rejects_unknown_pubkey() {
    let allowed_keys = Keys::generate();
    let policy = PolicyConfig {
        write: WritePolicy {
            allowed_pubkeys: Some(vec![allowed_keys.public_key().to_string()]),
            ..Default::default()
        },
        ..Default::default()
    };
    let (port, _store) = spawn_relay(policy).await;
    let mut client = WsTestClient::connect(port).await;

    let unknown_keys = Keys::generate();
    let event = make_event(&unknown_keys, "hello");
    client.send_event(&event).await;

    let (status, msg) = client.expect_ok().await;
    assert!(!status, "should reject unknown pubkey");
    assert!(msg.contains("blocked"), "message should contain 'blocked': {}", msg);
}

#[tokio::test]
async fn allow_list_accepts_listed_pubkey() {
    let keys = Keys::generate();
    let policy = PolicyConfig {
        write: WritePolicy {
            allowed_pubkeys: Some(vec![keys.public_key().to_string()]),
            ..Default::default()
        },
        ..Default::default()
    };
    let (port, _store) = spawn_relay(policy).await;
    let mut client = WsTestClient::connect(port).await;

    let event = make_event(&keys, "hello");
    client.send_event(&event).await;

    let (status, _msg) = client.expect_ok().await;
    assert!(status, "should accept listed pubkey");
}

#[tokio::test]
async fn blocked_kind_returns_ok_false() {
    let policy = PolicyConfig {
        events: EventPolicy {
            blocked_kinds: Some(vec![1]),
            ..Default::default()
        },
        ..Default::default()
    };
    let (port, _store) = spawn_relay(policy).await;
    let mut client = WsTestClient::connect(port).await;

    let keys = Keys::generate();
    let event = make_event(&keys, "hello"); // kind 1
    client.send_event(&event).await;

    let (status, msg) = client.expect_ok().await;
    assert!(!status, "should reject blocked kind");
    assert!(msg.contains("blocked"), "message should contain 'blocked': {}", msg);
}

#[tokio::test]
async fn content_too_long_returns_ok_false() {
    let policy = PolicyConfig {
        events: EventPolicy {
            max_content_length: Some(5),
            ..Default::default()
        },
        ..Default::default()
    };
    let (port, _store) = spawn_relay(policy).await;
    let mut client = WsTestClient::connect(port).await;

    let keys = Keys::generate();
    let event = make_event(&keys, "this is way too long");
    client.send_event(&event).await;

    let (status, msg) = client.expect_ok().await;
    assert!(!status, "should reject too-long content");
    assert!(msg.contains("blocked"), "message should contain 'blocked': {}", msg);
}

#[tokio::test]
async fn auth_required_returns_ok_false() {
    let policy = PolicyConfig {
        write: WritePolicy {
            require_auth: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (port, _store) = spawn_relay(policy).await;
    let mut client = WsTestClient::connect(port).await;

    let keys = Keys::generate();
    let event = make_event(&keys, "hello");
    client.send_event(&event).await;

    let (status, msg) = client.expect_ok().await;
    assert!(!status, "should reject unauthenticated");
    assert!(
        msg.contains("auth-required"),
        "message should contain 'auth-required': {}",
        msg
    );
}
