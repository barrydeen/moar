use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderMap},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use nostr::{ClientMessage, Event, JsonUtil, RelayMessage};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

use crate::config::RelayConfig;
use crate::policy::{PolicyEngine, PolicyResult};
use crate::stats::RelayStats;
use crate::storage::NostrStore;

pub struct RelayState {
    pub store: Arc<dyn NostrStore>,
    pub policy: Arc<PolicyEngine>,
    pub config: RelayConfig,
    pub relay_id: String,
    pub pages_dir: PathBuf,
    pub admin_pubkey: String,
    pub relay_url: String,
    pub tx: broadcast::Sender<Event>,
    pub stats: Arc<RelayStats>,
}

impl RelayState {
    pub fn new(
        config: RelayConfig,
        store: Arc<dyn NostrStore>,
        policy: Arc<PolicyEngine>,
        relay_id: String,
        pages_dir: PathBuf,
        admin_pubkey: String,
        relay_url: String,
        stats: Arc<RelayStats>,
    ) -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            store,
            policy,
            config,
            relay_id,
            pages_dir,
            admin_pubkey,
            relay_url,
            tx,
            stats,
        }
    }
}

struct ConnectionGuard(Arc<RelayStats>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.active_connections.fetch_sub(1, Relaxed);
    }
}

pub fn create_relay_router(state: Arc<RelayState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::OPTIONS])
        .allow_headers(Any);

    Router::new()
        .route("/", get(root_handler))
        .layer(cors)
        .with_state(state)
}

/// Handles NIP-11 info document, WebSocket upgrades, and regular HTTP GET requests.
async fn root_handler(
    ws: Option<WebSocketUpgrade>,
    headers: HeaderMap,
    State(state): State<Arc<RelayState>>,
) -> impl IntoResponse {
    // NIP-11: Return relay info document if client requests it
    if let Some(accept) = headers.get(header::ACCEPT) {
        if let Ok(accept_str) = accept.to_str() {
            if accept_str.contains("application/nostr+json") {
                let doc = build_nip11(&state);
                let json = serde_json::to_string(&doc).unwrap_or_default();
                return (
                    [(header::CONTENT_TYPE, "application/nostr+json")],
                    json,
                )
                    .into_response();
            }
        }
    }

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

// --- NIP-11 Relay Information Document ---

#[derive(Serialize)]
struct Nip11Document {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pubkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contact: Option<String>,
    supported_nips: Vec<u32>,
    software: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terms_of_service: Option<String>,
    limitation: Nip11Limitation,
}

#[derive(Serialize)]
struct Nip11Limitation {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_message_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_subscriptions: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_subid_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_content_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_event_tags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_pow_difficulty: Option<u8>,
    auth_required: bool,
    restricted_writes: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at_lower_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at_upper_limit: Option<u64>,
}

fn build_nip11(state: &RelayState) -> Nip11Document {
    let policy = &state.config.policy;
    let nip11 = &state.config.nip11;

    let auth_required = policy.write.require_auth || policy.read.require_auth;
    let restricted_writes = policy.write.allowed_pubkeys.is_some()
        || policy.write.wot.is_some()
        || policy.write.tagged_pubkeys.is_some();

    let pubkey = if state.admin_pubkey.is_empty() {
        None
    } else {
        Some(state.admin_pubkey.clone())
    };

    Nip11Document {
        name: state.config.name.clone(),
        description: state.config.description.clone(),
        pubkey,
        contact: nip11.contact.clone(),
        supported_nips: vec![1, 11, 13],
        software: "https://github.com/barrydeen/moar".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        icon: nip11.icon.clone(),
        banner: nip11.banner.clone(),
        terms_of_service: nip11.terms_of_service.clone(),
        limitation: Nip11Limitation {
            max_message_length: nip11.max_message_length,
            max_subscriptions: nip11.max_subscriptions,
            max_subid_length: nip11.max_subid_length,
            max_limit: nip11.max_limit,
            max_content_length: policy.events.max_content_length.map(|v| v as u64),
            max_event_tags: nip11.max_event_tags,
            default_limit: nip11.default_limit,
            min_pow_difficulty: policy.events.min_pow,
            auth_required,
            restricted_writes,
            created_at_lower_limit: nip11.created_at_lower_limit,
            created_at_upper_limit: nip11.created_at_upper_limit,
        },
    }
}

async fn send_msg(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: String,
    stats: &RelayStats,
) {
    stats.bytes_tx.fetch_add(msg.len() as u64, Relaxed);
    let _ = sender.send(Message::Text(msg.into())).await;
}

async fn handle_socket(socket: WebSocket, state: Arc<RelayState>) {
    let (mut sender, mut receiver) = socket.split();
    let stats = &state.stats;

    // Connection tracking
    stats.active_connections.fetch_add(1, Relaxed);
    stats.total_connections.fetch_add(1, Relaxed);
    let _guard = ConnectionGuard(stats.clone());

    // NIP-42: the authenticated pubkey for this connection (None until AUTH)
    let authed_pubkey: Option<nostr::PublicKey> = None;

    let mut broadcast_rx = state.tx.subscribe();

    loop {
        tokio::select! {
            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        stats.bytes_rx.fetch_add(text.len() as u64, Relaxed);
                        match ClientMessage::from_json(&text) {
                            Ok(client_msg) => {
                                match client_msg {
                                    ClientMessage::Event(event) => {
                                        match state.policy.can_write(&event, authed_pubkey.as_ref()) {
                                            PolicyResult::Allow => {
                                                if let Err(e) = state.store.save_event(&event) {
                                                    tracing::error!("Failed to save event: {}", e);
                                                    send_msg(&mut sender, RelayMessage::ok(event.id, false, "error saving").as_json(), stats).await;
                                                } else {
                                                    stats.events_saved.fetch_add(1, Relaxed);
                                                    send_msg(&mut sender, RelayMessage::ok(event.id, true, "").as_json(), stats).await;
                                                    let _ = state.tx.send(event.as_ref().clone());
                                                }
                                            }
                                            PolicyResult::Deny(reason) => {
                                                stats.events_rejected.fetch_add(1, Relaxed);
                                                send_msg(&mut sender, RelayMessage::ok(event.id, false, &format!("blocked: {}", reason)).as_json(), stats).await;
                                            }
                                            PolicyResult::AuthRequired => {
                                                send_msg(&mut sender, RelayMessage::ok(event.id, false, "auth-required: NIP-42 authentication required").as_json(), stats).await;
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
                                                    send_msg(&mut sender, RelayMessage::notice(format!("blocked: {}", reason)).as_json(), stats).await;
                                                    blocked = true;
                                                    break;
                                                }
                                                PolicyResult::AuthRequired => {
                                                    send_msg(&mut sender, RelayMessage::notice("auth-required: NIP-42 authentication required").as_json(), stats).await;
                                                    blocked = true;
                                                    break;
                                                }
                                            }
                                        }

                                        if !blocked {
                                            for filter in filters {
                                                match state.store.query(&filter) {
                                                    Ok(events) => {
                                                        stats.queries_served.fetch_add(1, Relaxed);
                                                        for event in events {
                                                            send_msg(&mut sender, RelayMessage::event(subscription_id.clone(), event).as_json(), stats).await;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        tracing::error!("Query failed: {}", e);
                                                        send_msg(&mut sender, RelayMessage::notice(format!("error: {}", e)).as_json(), stats).await;
                                                    }
                                                }
                                            }
                                            send_msg(&mut sender, RelayMessage::eose(subscription_id).as_json(), stats).await;
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
