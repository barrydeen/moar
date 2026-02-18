mod common;

use common::{spawn_relay, WsTestClient};
use moar::config::{PolicyConfig, ReadPolicy};
use moar::storage::NostrStore;
use nostr::{EventBuilder, Filter, Keys};

fn make_event(keys: &Keys, content: &str) -> nostr::Event {
    EventBuilder::text_note(content, [])
        .to_event(keys)
        .unwrap()
}

#[tokio::test]
async fn store_event_then_req_by_id_returns_event_and_eose() {
    let (port, store) = spawn_relay(PolicyConfig::default()).await;

    // Pre-populate the store
    let keys = Keys::generate();
    let event = make_event(&keys, "stored event");
    store.save_event(&event).unwrap();

    let mut client = WsTestClient::connect(port).await;

    let filter = Filter::new().id(event.id);
    client.send_req("sub1", vec![filter]).await;

    let received = client.expect_event().await;
    assert_eq!(received.id, event.id);

    client.expect_eose().await;
}

#[tokio::test]
async fn req_on_empty_store_returns_only_eose() {
    let (port, _store) = spawn_relay(PolicyConfig::default()).await;
    let mut client = WsTestClient::connect(port).await;

    let filter = Filter::new();
    client.send_req("sub1", vec![filter]).await;

    client.expect_eose().await;
}

#[tokio::test]
async fn read_require_auth_returns_notice_auth_required() {
    let policy = PolicyConfig {
        read: ReadPolicy {
            require_auth: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (port, _store) = spawn_relay(policy).await;
    let mut client = WsTestClient::connect(port).await;

    let filter = Filter::new();
    client.send_req("sub1", vec![filter]).await;

    let notice = client.expect_notice().await;
    assert!(
        notice.contains("auth-required"),
        "notice should contain 'auth-required': {}",
        notice
    );
}

#[tokio::test]
async fn read_allow_list_without_auth_returns_notice_blocked() {
    let keys = Keys::generate();
    let policy = PolicyConfig {
        read: ReadPolicy {
            require_auth: false,
            allowed_pubkeys: Some(vec![keys.public_key().to_string()]),
            wot: None,
            paywall: None,
        },
        ..Default::default()
    };
    let (port, _store) = spawn_relay(policy).await;
    let mut client = WsTestClient::connect(port).await;

    let filter = Filter::new();
    client.send_req("sub1", vec![filter]).await;

    let notice = client.expect_notice().await;
    assert!(
        notice.contains("blocked"),
        "notice should contain 'blocked': {}",
        notice
    );
}
