use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, Request, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use nostr::{ClientMessage, Event, JsonUtil, PublicKey, RelayMessage};
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

use crate::config::RelayConfig;
use crate::paywall::PaywallManager;
use crate::policy::{PolicyEngine, PolicyResult};
use crate::rate_limit::IpTracker;
use crate::stats::RelayStats;
use crate::storage::NostrStore;
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::atomic::Ordering::Relaxed;

pub struct RelayState {
    pub store: Arc<dyn NostrStore>,
    pub policy: Arc<PolicyEngine>,
    pub config: RelayConfig,
    pub relay_id: String,
    pub pages_dir: PathBuf,
    pub admin_pubkey: String,
    pub relay_url: String,
    pub tx: broadcast::Sender<Event>,
    pub paywall_manager: Option<Arc<PaywallManager>>,
    pub paywall_id: Option<String>,
    pub stats: Arc<RelayStats>,
    pub ip_tracker: Arc<IpTracker>,
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
        paywall_manager: Option<Arc<PaywallManager>>,
        paywall_id: Option<String>,
        stats: Arc<RelayStats>,
        ip_tracker: Arc<IpTracker>,
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
            paywall_manager,
            paywall_id,
            stats,
            ip_tracker,
        }
    }
}

pub fn create_relay_router(state: Arc<RelayState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(Any);

    Router::new()
        .route("/", get(root_handler))
        .route("/checkout/info", get(checkout_info_handler))
        .route("/checkout", post(checkout_handler))
        .route("/checkout/status", get(checkout_status_handler))
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

    // Extract client IP from X-Forwarded-For header or fall back to loopback
    let client_ip = extract_client_ip(&headers);

    // WebSocket upgrade takes priority
    if let Some(ws) = ws {
        // Enforce per-IP connection limit
        let max_conn = state.config.policy.rate_limit.max_connections;
        if !state.ip_tracker.try_connect(client_ip, max_conn) {
            return (StatusCode::SERVICE_UNAVAILABLE, "too many connections from your IP").into_response();
        }
        let ip = client_ip;
        return ws.on_upgrade(move |socket| handle_socket(socket, state, ip)).into_response();
    }

    // Serve custom home page if it exists
    let page_path = state.pages_dir.join(format!("{}.html", state.relay_id));
    if let Ok(content) = tokio::fs::read_to_string(&page_path).await {
        return Html(content).into_response();
    }

    // If this relay has a paywall, serve the checkout page
    if let (Some(ref pm), Some(ref pw_id)) = (&state.paywall_manager, &state.paywall_id) {
        if let Some(info) = pm.get_paywall_info(pw_id).await {
            let access_mode = determine_access_mode(&state.config);
            let template = include_str!("web/checkout.html");
            let html = template
                .replace("{{RELAY_NAME}}", &html_escape(&state.config.name))
                .replace("{{PRICE_SATS}}", &info.price_sats.to_string())
                .replace("{{PERIOD_DAYS}}", &info.period_days.to_string())
                .replace("{{ACCESS_MODE}}", access_mode);
            return Html(html).into_response();
        }
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

/// Extract client IP from X-Forwarded-For header, falling back to loopback.
fn extract_client_ip(headers: &HeaderMap) -> IpAddr {
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(xff_str) = xff.to_str() {
            // Take the first (leftmost) IP — the original client
            if let Some(first) = xff_str.split(',').next() {
                if let Ok(ip) = first.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    // Fallback — peer address not available in this handler, use loopback
    IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
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
    payment_required: bool,
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
        || policy.write.tagged_pubkeys.is_some()
        || policy.write.paywall.is_some();
    let payment_required = policy.write.paywall.is_some() || policy.read.paywall.is_some();

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
            payment_required,
            created_at_lower_limit: nip11.created_at_lower_limit,
            created_at_upper_limit: nip11.created_at_upper_limit,
        },
    }
}

fn determine_access_mode(config: &RelayConfig) -> &'static str {
    let has_write = config.policy.write.paywall.is_some();
    let has_read = config.policy.read.paywall.is_some();
    match (has_write, has_read) {
        (true, true) => "read and write",
        (true, false) => "write",
        (false, true) => "read",
        (false, false) => "none",
    }
}

// --- Checkout Handlers ---

#[derive(Serialize)]
struct CheckoutInfoResponse {
    price_sats: u64,
    period_days: u32,
    access_mode: String,
    relay_name: String,
}

async fn checkout_info_handler(
    State(state): State<Arc<RelayState>>,
) -> impl IntoResponse {
    let (pm, pw_id) = match (&state.paywall_manager, &state.paywall_id) {
        (Some(pm), Some(id)) => (pm, id),
        _ => return (StatusCode::NOT_FOUND, "No paywall configured").into_response(),
    };

    match pm.get_paywall_info(pw_id).await {
        Some(info) => Json(CheckoutInfoResponse {
            price_sats: info.price_sats,
            period_days: info.period_days,
            access_mode: determine_access_mode(&state.config).to_string(),
            relay_name: state.config.name.clone(),
        })
        .into_response(),
        None => (StatusCode::NOT_FOUND, "Paywall not found").into_response(),
    }
}

#[derive(Deserialize)]
struct CheckoutRequest {
    npub: String,
}

#[derive(Serialize)]
struct CheckoutResponse {
    invoice: String,
    payment_hash: String,
    amount_sats: u64,
    qr_svg: String,
}

fn generate_qr_svg(data: &str) -> String {
    use qrcode::QrCode;
    let code = QrCode::new(data.to_uppercase().as_bytes()).unwrap();
    code.render::<qrcode::render::svg::Color>()
        .quiet_zone(true)
        .min_dimensions(256, 256)
        .build()
}

async fn checkout_handler(
    State(state): State<Arc<RelayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    let (pm, pw_id) = match (&state.paywall_manager, &state.paywall_id) {
        (Some(pm), Some(id)) => (pm, id),
        _ => return (StatusCode::NOT_FOUND, "No paywall configured").into_response(),
    };

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let payload: CheckoutRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    // Parse npub or hex pubkey
    let pubkey = match PublicKey::parse(&payload.npub) {
        Ok(pk) => pk,
        Err(_) => match PublicKey::from_str(&payload.npub) {
            Ok(pk) => pk,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid pubkey: {}", e),
                )
                    .into_response()
            }
        },
    };

    match pm.create_invoice(pw_id, pubkey).await {
        Ok(invoice_resp) => {
            let info = pm.get_paywall_info(pw_id).await;
            let amount_sats = info.map(|i| i.price_sats).unwrap_or(0);
            let qr_svg = generate_qr_svg(&invoice_resp.invoice);
            Json(CheckoutResponse {
                invoice: invoice_resp.invoice,
                payment_hash: invoice_resp.payment_hash,
                amount_sats,
                qr_svg,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create invoice: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to create invoice: {}", e) })),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
struct CheckoutStatusQuery {
    payment_hash: String,
}

#[derive(Serialize)]
struct CheckoutStatusResponse {
    status: String,
}

async fn checkout_status_handler(
    Query(query): Query<CheckoutStatusQuery>,
    State(state): State<Arc<RelayState>>,
) -> impl IntoResponse {
    let (pm, pw_id) = match (&state.paywall_manager, &state.paywall_id) {
        (Some(pm), Some(id)) => (pm, id),
        _ => return (StatusCode::NOT_FOUND, "No paywall configured").into_response(),
    };

    match pm.check_payment(pw_id, &query.payment_hash).await {
        Ok(status) => {
            let status_str = match status {
                crate::nwc::InvoiceStatus::Pending => "pending",
                crate::nwc::InvoiceStatus::Paid => "paid",
                crate::nwc::InvoiceStatus::Expired => "expired",
            };
            Json(CheckoutStatusResponse {
                status: status_str.to_string(),
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to check payment: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to check payment: {}", e) })),
            )
                .into_response()
        }
    }
}

// --- WebSocket Handler ---

struct ConnectionGuard {
    stats: Arc<RelayStats>,
    ip_tracker: Arc<IpTracker>,
    ip: IpAddr,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.stats.active_connections.fetch_sub(1, Relaxed);
        self.ip_tracker.disconnect(self.ip);
    }
}

async fn send_msg(sender: &mut futures::stream::SplitSink<WebSocket, Message>, msg: String, stats: &RelayStats) {
    stats.bytes_tx.fetch_add(msg.len() as u64, Relaxed);
    let _ = sender.send(Message::Text(msg.into())).await;
}

async fn handle_socket(socket: WebSocket, state: Arc<RelayState>, client_ip: IpAddr) {
    let (mut sender, mut receiver) = socket.split();

    let stats = &state.stats;
    stats.active_connections.fetch_add(1, Relaxed);
    stats.total_connections.fetch_add(1, Relaxed);
    let _guard = ConnectionGuard {
        stats: stats.clone(),
        ip_tracker: state.ip_tracker.clone(),
        ip: client_ip,
    };

    let nip11 = &state.config.nip11;
    let rate_limit = &state.config.policy.rate_limit;

    // NIP-11: max_message_length for raw text check
    let max_message_length = nip11.max_message_length.unwrap_or(524288) as usize;

    // NIP-11: subscription limits (per-connection)
    let max_subscriptions = nip11.max_subscriptions.unwrap_or(20) as usize;
    let max_subid_length = nip11.max_subid_length.unwrap_or(64) as usize;
    let max_limit = nip11.max_limit;
    let default_limit = nip11.default_limit;

    // NIP-42: the authenticated pubkey for this connection (None until AUTH)
    let authed_pubkey: Option<nostr::PublicKey> = None;

    // Track active subscriptions for this connection
    let mut active_subs: HashSet<String> = HashSet::new();

    let mut broadcast_rx = state.tx.subscribe();

    loop {
        tokio::select! {
            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        stats.bytes_rx.fetch_add(text.len() as u64, Relaxed);

                        // NIP-11: max_message_length check before parsing
                        if text.len() > max_message_length {
                            send_msg(&mut sender, RelayMessage::notice(
                                format!("message too large ({} > {})", text.len(), max_message_length)
                            ).as_json(), stats).await;
                            continue;
                        }

                        match ClientMessage::from_json(&text) {
                            Ok(client_msg) => {
                                match client_msg {
                                    ClientMessage::Event(event) => {
                                        // Per-IP write rate limit
                                        if !state.ip_tracker.check_write_rate(client_ip, rate_limit.writes_per_minute) {
                                            send_msg(&mut sender, RelayMessage::ok(event.id, false, "rate-limited: too many writes per minute").as_json(), stats).await;
                                            continue;
                                        }

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
                                        let sub_id_str = subscription_id.to_string();

                                        // NIP-11: max_subid_length
                                        if sub_id_str.len() > max_subid_length {
                                            send_msg(&mut sender, RelayMessage::notice(
                                                format!("subscription ID too long ({} > {})", sub_id_str.len(), max_subid_length)
                                            ).as_json(), stats).await;
                                            continue;
                                        }

                                        // NIP-11: max_subscriptions (only count genuinely new subs)
                                        if !active_subs.contains(&sub_id_str) && active_subs.len() >= max_subscriptions {
                                            send_msg(&mut sender, RelayMessage::notice(
                                                format!("too many subscriptions ({} max)", max_subscriptions)
                                            ).as_json(), stats).await;
                                            continue;
                                        }

                                        // Per-IP read rate limit
                                        if !state.ip_tracker.check_read_rate(client_ip, rate_limit.reads_per_minute) {
                                            send_msg(&mut sender, RelayMessage::notice("rate-limited: too many reads per minute").as_json(), stats).await;
                                            continue;
                                        }

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
                                            active_subs.insert(sub_id_str);

                                            for filter in filters {
                                                // NIP-11: clamp filter limit
                                                let mut clamped_filter = filter;
                                                match clamped_filter.limit {
                                                    Some(l) => {
                                                        if let Some(max) = max_limit {
                                                            if l as u64 > max {
                                                                clamped_filter.limit = Some(max as usize);
                                                            }
                                                        }
                                                    }
                                                    None => {
                                                        if let Some(def) = default_limit {
                                                            clamped_filter.limit = Some(def as usize);
                                                        }
                                                    }
                                                }

                                                match state.store.query(&clamped_filter) {
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
                                    ClientMessage::Close(sub_id) => {
                                        active_subs.remove(&sub_id.to_string());
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
