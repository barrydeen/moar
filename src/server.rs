use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use nostr::{ClientMessage, Event, JsonUtil, RelayMessage};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::config::RelayConfig;
use crate::policy::{PolicyEngine, PolicyResult};
use crate::storage::NostrStore;

pub struct RelayState {
    pub store: Arc<dyn NostrStore>,
    pub policy: Arc<PolicyEngine>,
    pub config: RelayConfig,
    pub relay_id: String,
    pub pages_dir: PathBuf,
    pub tx: broadcast::Sender<Event>,
}

impl RelayState {
    pub fn new(
        config: RelayConfig,
        store: Arc<dyn NostrStore>,
        policy: Arc<PolicyEngine>,
        relay_id: String,
        pages_dir: PathBuf,
    ) -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            store,
            policy,
            config,
            relay_id,
            pages_dir,
            tx,
        }
    }
}

pub fn create_relay_router(state: Arc<RelayState>) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .with_state(state)
}

/// Handles both WebSocket upgrades and regular HTTP GET requests.
/// If the request is a WebSocket upgrade, hand off to the WS handler.
/// Otherwise, serve the relay's custom home page (or a default).
async fn root_handler(
    ws: Option<WebSocketUpgrade>,
    State(state): State<Arc<RelayState>>,
) -> impl IntoResponse {
    // WebSocket upgrade takes priority
    if let Some(ws) = ws {
        return ws.on_upgrade(|socket| handle_socket(socket, state)).into_response();
    }

    // Serve custom home page if it exists
    let page_path = state.pages_dir.join(format!("{}.html", state.relay_id));
    if let Ok(content) = tokio::fs::read_to_string(&page_path).await {
        return Html(content).into_response();
    }

    // Default relay info page
    let name = html_escape(&state.config.name);
    let desc = state
        .config
        .description
        .as_deref()
        .unwrap_or("A Nostr relay powered by MOAR");
    let desc = html_escape(desc);

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{name}</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{background:#0a0a0a;color:#fff;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;display:flex;align-items:center;justify-content:center;min-height:100vh}}
.container{{text-align:center;max-width:480px;padding:2rem}}
h1{{font-size:1.5rem;margin-bottom:0.5rem}}
p{{color:#888;font-size:0.95rem;line-height:1.5}}
.badge{{display:inline-block;background:#1a1a2e;border:1px solid #333;border-radius:9999px;padding:0.25rem 0.75rem;font-size:0.75rem;color:#aaa;margin-top:1rem;font-family:monospace}}
</style>
</head>
<body>
<div class="container">
<h1>{name}</h1>
<p>{desc}</p>
<span class="badge">Nostr Relay</span>
</div>
</body>
</html>"#
    );

    Html(html).into_response()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
