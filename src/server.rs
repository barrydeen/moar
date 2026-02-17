use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use nostr::{ClientMessage, Event, JsonUtil, RelayMessage};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::config::RelayConfig;
use crate::policy::{PolicyEngine, PolicyResult};
use crate::storage::NostrStore;

pub struct RelayState {
    pub store: Arc<dyn NostrStore>,
    pub policy: Arc<PolicyEngine>,
    pub config: RelayConfig,
    pub tx: broadcast::Sender<Event>,
}

impl RelayState {
    pub fn new(config: RelayConfig, store: Arc<dyn NostrStore>, policy: Arc<PolicyEngine>) -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            store,
            policy,
            config,
            tx,
        }
    }
}

pub fn create_relay_router(state: Arc<RelayState>) -> Router {
    Router::new()
        .route("/", get(websocket_handler))
        .with_state(state)
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RelayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<RelayState>) {
    let (mut sender, mut receiver) = socket.split();

    // NIP-42: the authenticated pubkey for this connection (None until AUTH)
    let authed_pubkey: Option<nostr::PublicKey> = None;

    let mut broadcast_rx = state.tx.subscribe();

    loop {
        tokio::select! {
            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        match ClientMessage::from_json(&text) {
                            Ok(client_msg) => {
                                match client_msg {
                                    ClientMessage::Event(event) => {
                                        match state.policy.can_write(&event, authed_pubkey.as_ref()) {
                                            PolicyResult::Allow => {
                                                if let Err(e) = state.store.save_event(&event) {
                                                    tracing::error!("Failed to save event: {}", e);
                                                    let _ = sender.send(Message::Text(RelayMessage::ok(event.id, false, "error saving").as_json().into())).await;
                                                } else {
                                                    let _ = sender.send(Message::Text(RelayMessage::ok(event.id, true, "").as_json().into())).await;
                                                    let _ = state.tx.send(event.as_ref().clone());
                                                }
                                            }
                                            PolicyResult::Deny(reason) => {
                                                let _ = sender.send(Message::Text(RelayMessage::ok(event.id, false, &format!("blocked: {}", reason)).as_json().into())).await;
                                            }
                                            PolicyResult::AuthRequired => {
                                                let _ = sender.send(Message::Text(RelayMessage::ok(event.id, false, "auth-required: NIP-42 authentication required").as_json().into())).await;
                                                // TODO: send AUTH challenge
                                            }
                                        }
                                    }
                                    ClientMessage::Req { subscription_id, filters } => {
                                        // Check read policy on each filter
                                        let mut blocked = false;
                                        for filter in &filters {
                                            match state.policy.can_read(filter, authed_pubkey.as_ref()) {
                                                PolicyResult::Allow => {}
                                                PolicyResult::Deny(reason) => {
                                                    let _ = sender.send(Message::Text(RelayMessage::notice(format!("blocked: {}", reason)).as_json().into())).await;
                                                    blocked = true;
                                                    break;
                                                }
                                                PolicyResult::AuthRequired => {
                                                    let _ = sender.send(Message::Text(RelayMessage::notice("auth-required: NIP-42 authentication required").as_json().into())).await;
                                                    blocked = true;
                                                    break;
                                                }
                                            }
                                        }

                                        if !blocked {
                                            for filter in filters {
                                                match state.store.query(&filter) {
                                                    Ok(events) => {
                                                        for event in events {
                                                            let _ = sender.send(Message::Text(RelayMessage::event(subscription_id.clone(), event).as_json().into())).await;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        tracing::error!("Query failed: {}", e);
                                                        let _ = sender.send(Message::Text(RelayMessage::notice(format!("error: {}", e)).as_json().into())).await;
                                                    }
                                                }
                                            }
                                            let _ = sender.send(Message::Text(RelayMessage::eose(subscription_id).as_json().into())).await;
                                        }
                                    }
                                    ClientMessage::Close(_sub_id) => {
                                        // subscriptions.remove(&sub_id);
                                    }
                                    _ => {}
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Invalid Nostr message: {}", e);
                            }
                        }
                    }
                    _ => {} // binary or other
                }
            }
            Ok(_event) = broadcast_rx.recv() => {
                // TODO: Matching logic
            }
        }
    }
}
