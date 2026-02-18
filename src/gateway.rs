use crate::auth::verify_auth_event;
use crate::blossom::handlers::{self as blossom_handlers, BlossomState};
use crate::blossom::store::BlobStore;
use crate::config::{BlossomConfig, MoarConfig, PaywallConfig, RelayConfig, WotConfig};
use crate::paywall::PaywallManager;
use crate::policy::PolicyEngine;
use crate::server::{self, RelayState};
use crate::storage::NostrStore;
use crate::wot::WotManager;
use axum::{
    body::Body,
    extract::{FromRequest, Host, Path, Query, Request, State},
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::{delete as delete_route, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tower::ServiceExt;

#[derive(Clone)]
pub struct GatewayState {
    pub domain: String,
    pub port: u16,
    pub relay_routers: HashMap<String, Router>,
    pub relay_configs: HashMap<String, RelayConfig>,
    pub relay_stores: HashMap<String, Arc<dyn NostrStore>>,
    pub blossom_routers: HashMap<String, Router>,
    pub blossom_stores: HashMap<String, Arc<BlobStore>>,
    pub config: Arc<RwLock<MoarConfig>>,
    pub config_path: PathBuf,
    pub pages_dir: PathBuf,
    pub pending_restart: Arc<RwLock<bool>>,
    pub sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    pub wot_manager: Arc<WotManager>,
    pub paywall_manager: Arc<PaywallManager>,
}

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub pubkey: String,
    pub created_at: u64,
}

impl SessionInfo {
    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.created_at > 24 * 60 * 60
    }
}

pub async fn start_gateway(
    port: u16,
    domain: String,
    relays: HashMap<String, (RelayConfig, Arc<dyn NostrStore>, Arc<PolicyEngine>)>,
    blossoms: HashMap<String, (BlossomConfig, Arc<BlobStore>)>,
    config: MoarConfig,
    config_path: PathBuf,
    wot_manager: Arc<WotManager>,
    paywall_manager: Arc<PaywallManager>,
) -> crate::error::Result<()> {
    let pages_dir = PathBuf::from(&config.pages_dir);
    // Ensure the pages directory exists
    let _ = tokio::fs::create_dir_all(&pages_dir).await;

    let mut router_map = HashMap::new();
    let mut config_map = HashMap::new();
    let mut store_map: HashMap<String, Arc<dyn NostrStore>> = HashMap::new();

    for (key, (relay_config, store, policy)) in relays {
        let scheme = if domain == "localhost" { "http" } else { "https" };
        let relay_url = format!(
            "{}://{}.{}",
            scheme, relay_config.subdomain, domain
        );
        store_map.insert(key.clone(), store.clone());

        // Determine paywall for this relay (write and read reference the same ID)
        let paywall_id = relay_config
            .policy
            .write
            .paywall
            .as_ref()
            .or(relay_config.policy.read.paywall.as_ref())
            .cloned();

        let state = Arc::new(RelayState::new(
            relay_config.clone(),
            store,
            policy,
            key.clone(),
            pages_dir.clone(),
            config.admin_pubkey.clone(),
            relay_url,
            paywall_id.as_ref().map(|_| paywall_manager.clone()),
            paywall_id,
        ));
        let app = server::create_relay_router(state);
        router_map.insert(relay_config.subdomain.clone(), app);
        config_map.insert(relay_config.subdomain.clone(), relay_config);
    }

    let mut blossom_router_map = HashMap::new();
    let mut blossom_store_map = HashMap::new();

    for (key, (blossom_config, store)) in blossoms {
        let scheme = if domain == "localhost" { "http" } else { "https" };
        let base_url = if domain == "localhost" {
            format!("{}://{}.{}:{}", scheme, blossom_config.subdomain, domain, port)
        } else {
            format!("{}://{}.{}", scheme, blossom_config.subdomain, domain)
        };
        let blossom_state = BlossomState {
            config: blossom_config.clone(),
            store: store.clone(),
            server_id: key.clone(),
            base_url,
        };
        let app = blossom_handlers::create_blossom_router(blossom_state);
        blossom_router_map.insert(blossom_config.subdomain.clone(), app);
        blossom_store_map.insert(key, store);
    }

    let state = Arc::new(GatewayState {
        domain: domain.clone(),
        port,
        relay_routers: router_map,
        relay_configs: config_map,
        relay_stores: store_map,
        blossom_routers: blossom_router_map,
        blossom_stores: blossom_store_map,
        config: Arc::new(RwLock::new(config)),
        config_path,
        pages_dir,
        pending_restart: Arc::new(RwLock::new(false)),
        sessions: Arc::new(RwLock::new(HashMap::new())),
        wot_manager,
        paywall_manager,
    });

    let app = Router::new().fallback(handler).with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(
        "Gateway listening on http://{}:{} (domain: {})",
        "0.0.0.0",
        port,
        domain
    );
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handler(
    State(state): State<Arc<GatewayState>>,
    Host(host): Host,
    _uri: Uri,
    request: Request<Body>,
) -> Response {
    let hostname = host.split(':').next().unwrap_or(&host);

    if hostname == state.domain || hostname == "localhost" {
        let router = admin_router().with_state(state.clone());
        match router.oneshot(request).await {
            Ok(res) => return res,
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Admin Router error").into_response()
            }
        }
    }

    if let Some(subdomain) = hostname.strip_suffix(&state.domain) {
        let sub = if subdomain.ends_with('.') {
            &subdomain[..subdomain.len() - 1]
        } else {
            subdomain
        };

        if let Some(router) = state.relay_routers.get(sub) {
            let router = router.clone();
            match router.oneshot(request).await {
                Ok(res) => return res,
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Router error").into_response();
                }
            }
        }

        if let Some(router) = state.blossom_routers.get(sub) {
            let router = router.clone();
            match router.oneshot(request).await {
                Ok(res) => return res,
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Router error").into_response();
                }
            }
        }
    }

    (
        StatusCode::NOT_FOUND,
        format!("Service not found for host: {}", hostname),
    )
        .into_response()
}

// --- Admin Router ---

pub fn admin_router() -> Router<Arc<GatewayState>> {
    Router::new()
        .route("/", get(serve_index))
        .route("/api/login", post(login_handler))
        .route("/api/logout", post(logout_handler))
        .route("/api/status", get(status_handler))
        .route("/api/relays", get(list_relays).post(create_relay))
        .route(
            "/api/relays/:id",
            get(get_relay).put(update_relay).delete(delete_relay),
        )
        .route(
            "/api/relays/:id/page",
            get(get_relay_page).put(put_relay_page).delete(delete_relay_page),
        )
        .route("/api/relays/:id/export", get(export_relay))
        .route("/api/relays/:id/import", post(import_relay))
        .route("/api/wots", get(list_wots).post(create_wot))
        .route(
            "/api/wots/:id",
            get(get_wot).put(update_wot).delete(delete_wot),
        )
        .route(
            "/api/discovery-relays",
            get(get_discovery_relays).put(put_discovery_relays),
        )
        .route("/api/blossoms", get(list_blossoms).post(create_blossom))
        .route(
            "/api/blossoms/:id",
            get(get_blossom)
                .put(update_blossom)
                .delete(delete_blossom),
        )
        .route("/api/blossoms/:id/media", get(list_blossom_media).post(upload_blossom_media))
        .route("/api/blossoms/:id/media/:sha256", delete_route(delete_blossom_media))
        .route("/api/paywalls", get(list_paywalls).post(create_paywall))
        .route(
            "/api/paywalls/:id",
            get(get_paywall).put(update_paywall).delete(delete_paywall),
        )
        .route("/api/paywalls/:id/verify-nwc", post(verify_nwc_handler))
        .route("/api/paywalls/:id/whitelist", get(get_paywall_whitelist))
        .route("/api/restart", post(restart_handler))
        .route("/api/update", post(update_handler))
        .route("/api/update-status", get(update_status_handler))
        .route("/.well-known/caddy-ask", get(caddy_ask_handler))
}

async fn serve_index() -> impl IntoResponse {
    Html(include_str!("web/index.html"))
}

// --- Auth helpers ---

fn extract_session_token(request_headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = request_headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some(value) = trimmed.strip_prefix("moar_session=") {
            return Some(value.to_string());
        }
    }
    None
}

async fn require_auth(
    headers: &axum::http::HeaderMap,
    sessions: &Arc<RwLock<HashMap<String, SessionInfo>>>,
) -> Result<String, Response> {
    let token = extract_session_token(headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, "Not authenticated").into_response()
    })?;

    let sessions_read = sessions.read().await;
    let session = sessions_read.get(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, "Invalid session").into_response()
    })?;

    if session.is_expired() {
        drop(sessions_read);
        sessions.write().await.remove(&token);
        return Err((StatusCode::UNAUTHORIZED, "Session expired").into_response());
    }

    Ok(session.pubkey.clone())
}

// --- Handlers ---

async fn login_handler(
    State(state): State<Arc<GatewayState>>,
    Json(event): Json<nostr::Event>,
) -> impl IntoResponse {
    if let Err(e) = verify_auth_event(&event, "/api/login", "POST") {
        return (StatusCode::UNAUTHORIZED, e).into_response();
    }

    let pubkey = event.author().to_hex();

    // Only the configured admin pubkey can log in
    let config = state.config.read().await;
    if pubkey != config.admin_pubkey {
        return (StatusCode::FORBIDDEN, "Not authorized as admin").into_response();
    }
    drop(config);

    let token = uuid::Uuid::new_v4().to_string();

    let session = SessionInfo {
        pubkey,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    state.sessions.write().await.insert(token.clone(), session);

    let cookie = format!(
        "moar_session={}; HttpOnly; Path=/; SameSite=Strict",
        token
    );

    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        "Logged in",
    )
        .into_response()
}

async fn logout_handler(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Some(token) = extract_session_token(request.headers()) {
        state.sessions.write().await.remove(&token);
    }

    let cookie = "moar_session=; HttpOnly; Path=/; SameSite=Strict; Max-Age=0";

    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie.to_string())],
        "Logged out",
    )
        .into_response()
}

#[derive(Serialize)]
struct StatusResponse {
    pending_restart: bool,
    domain: String,
    port: u16,
}

async fn status_handler(State(state): State<Arc<GatewayState>>) -> impl IntoResponse {
    let pending = *state.pending_restart.read().await;
    Json(StatusResponse {
        pending_restart: pending,
        domain: state.domain.clone(),
        port: state.port,
    })
}

#[derive(Serialize)]
struct RelayResponse {
    id: String,
    #[serde(flatten)]
    config: RelayConfig,
}

async fn list_relays(State(state): State<Arc<GatewayState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let relays: Vec<RelayResponse> = config
        .relays
        .iter()
        .map(|(id, cfg)| RelayResponse {
            id: id.clone(),
            config: cfg.clone(),
        })
        .collect();
    Json(relays)
}

async fn get_relay(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    match config.relays.get(&id) {
        Some(cfg) => Json(RelayResponse {
            id: id.clone(),
            config: cfg.clone(),
        })
        .into_response(),
        None => (StatusCode::NOT_FOUND, "Relay not found").into_response(),
    }
}

fn validate_relay_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("ID cannot be empty".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("ID must contain only alphanumeric characters, hyphens, and underscores".to_string());
    }
    Ok(())
}

fn validate_relay_config(
    config: &RelayConfig,
    existing_relays: &HashMap<String, RelayConfig>,
    existing_blossoms: &HashMap<String, BlossomConfig>,
    exclude_id: Option<&str>,
) -> Result<(), String> {
    if config.name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if config.subdomain.is_empty() {
        return Err("Subdomain cannot be empty".to_string());
    }
    // Check subdomain uniqueness across relays and blossoms
    for (id, existing) in existing_relays {
        if Some(id.as_str()) == exclude_id {
            continue;
        }
        if existing.subdomain == config.subdomain {
            return Err(format!(
                "Subdomain '{}' is already used by relay '{}'",
                config.subdomain, id
            ));
        }
    }
    for (id, existing) in existing_blossoms {
        if existing.subdomain == config.subdomain {
            return Err(format!(
                "Subdomain '{}' is already used by blossom server '{}'",
                config.subdomain, id
            ));
        }
    }
    Ok(())
}

async fn save_config(state: &GatewayState, config: &MoarConfig) -> Result<(), Response> {
    let toml_str = toml::to_string_pretty(config).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize config: {}", e),
        )
            .into_response()
    })?;

    tokio::fs::write(&state.config_path, toml_str)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to write config: {}", e),
            )
                .into_response()
        })?;

    *state.pending_restart.write().await = true;
    Ok(())
}

async fn create_relay(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = axum::body::to_bytes(request.into_body(), 1024 * 64)
        .await
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid body").into_response())
        .unwrap();

    #[derive(serde::Deserialize)]
    struct CreateRelayRequest {
        id: String,
        #[serde(flatten)]
        config: RelayConfig,
    }

    let payload: CreateRelayRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if let Err(e) = validate_relay_id(&payload.id) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let mut config = state.config.write().await;

    if config.relays.contains_key(&payload.id) {
        return (
            StatusCode::CONFLICT,
            format!("Relay '{}' already exists", payload.id),
        )
            .into_response();
    }

    if let Err(e) = validate_relay_config(&payload.config, &config.relays, &config.blossoms, None) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    config.relays.insert(payload.id.clone(), payload.config.clone());

    if let Err(resp) = save_config(&state, &config).await {
        // Rollback
        config.relays.remove(&payload.id);
        return resp;
    }

    (
        StatusCode::CREATED,
        Json(RelayResponse {
            id: payload.id,
            config: payload.config,
        }),
    )
        .into_response()
}

async fn update_relay(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = axum::body::to_bytes(request.into_body(), 1024 * 64)
        .await
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid body").into_response())
        .unwrap();

    let new_config: RelayConfig = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    let mut config = state.config.write().await;

    if !config.relays.contains_key(&id) {
        return (StatusCode::NOT_FOUND, "Relay not found").into_response();
    }

    if let Err(e) = validate_relay_config(&new_config, &config.relays, &config.blossoms, Some(&id)) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let old_config = config.relays.insert(id.clone(), new_config.clone());

    if let Err(resp) = save_config(&state, &config).await {
        // Rollback
        if let Some(old) = old_config {
            config.relays.insert(id.clone(), old);
        }
        return resp;
    }

    Json(RelayResponse {
        id,
        config: new_config,
    })
    .into_response()
}

async fn delete_relay(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let mut config = state.config.write().await;

    let removed = config.relays.remove(&id);
    if removed.is_none() {
        return (StatusCode::NOT_FOUND, "Relay not found").into_response();
    }

    if let Err(resp) = save_config(&state, &config).await {
        // Rollback
        if let Some(old) = removed {
            config.relays.insert(id, old);
        }
        return resp;
    }

    // Clean up custom page file if it exists
    let page_path = state.pages_dir.join(format!("{}.html", id));
    let _ = tokio::fs::remove_file(&page_path).await;

    StatusCode::NO_CONTENT.into_response()
}

// --- Relay Page Handlers ---

fn sanitize_relay_id_for_path(id: &str) -> Result<(), Response> {
    // Prevent path traversal
    if id.contains('.') || id.contains('/') || id.contains('\\') {
        return Err((StatusCode::BAD_REQUEST, "Invalid relay ID").into_response());
    }
    Ok(())
}

async fn get_relay_page(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = sanitize_relay_id_for_path(&id) {
        return resp;
    }

    // Verify relay exists
    let config = state.config.read().await;
    if !config.relays.contains_key(&id) {
        return (StatusCode::NOT_FOUND, "Relay not found").into_response();
    }
    drop(config);

    let page_path = state.pages_dir.join(format!("{}.html", id));
    match tokio::fs::read_to_string(&page_path).await {
        Ok(content) => Json(serde_json::json!({ "html": content })).into_response(),
        Err(_) => Json(serde_json::json!({ "html": serde_json::Value::Null })).into_response(),
    }
}

#[derive(Deserialize)]
struct PagePayload {
    html: String,
}

async fn put_relay_page(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    if let Err(resp) = sanitize_relay_id_for_path(&id) {
        return resp;
    }

    // Verify relay exists
    let config = state.config.read().await;
    if !config.relays.contains_key(&id) {
        return (StatusCode::NOT_FOUND, "Relay not found").into_response();
    }
    drop(config);

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 512).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Body too large (max 512KB)").into_response(),
    };

    let payload: PagePayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response(),
    };

    // Ensure pages directory exists
    let _ = tokio::fs::create_dir_all(&state.pages_dir).await;

    let page_path = state.pages_dir.join(format!("{}.html", id));
    match tokio::fs::write(&page_path, &payload.html).await {
        Ok(_) => (StatusCode::OK, "Page saved").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write page: {}", e),
        )
            .into_response(),
    }
}

async fn delete_relay_page(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    if let Err(resp) = sanitize_relay_id_for_path(&id) {
        return resp;
    }

    let page_path = state.pages_dir.join(format!("{}.html", id));
    let _ = tokio::fs::remove_file(&page_path).await;

    StatusCode::NO_CONTENT.into_response()
}

// --- Relay Import/Export Handlers ---

async fn export_relay(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let store = match state.relay_stores.get(&id) {
        Some(s) => s.clone(),
        None => return (StatusCode::NOT_FOUND, "Relay not found").into_response(),
    };

    let events = match store.iter_all() {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read events: {}", e),
            )
                .into_response()
        }
    };

    let mut body = String::new();
    for event in &events {
        if let Ok(json) = serde_json::to_string(event) {
            body.push_str(&json);
            body.push('\n');
        }
    }

    let filename = format!("{}.jsonl", id);
    (
        [
            (header::CONTENT_TYPE, "application/jsonl".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        body,
    )
        .into_response()
}

#[derive(Serialize)]
struct ImportResult {
    imported: usize,
    skipped: usize,
    errors: usize,
}

async fn import_relay(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let store = match state.relay_stores.get(&id) {
        Some(s) => s.clone(),
        None => return (StatusCode::NOT_FOUND, "Relay not found").into_response(),
    };

    let mut multipart = match axum::extract::Multipart::from_request(request, &()).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::BAD_REQUEST, "Expected multipart form data").into_response(),
    };

    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return (StatusCode::BAD_REQUEST, "No file field found").into_response(),
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid multipart data").into_response(),
    };

    let data = match field.bytes().await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to read file data").into_response(),
    };

    let content = match String::from_utf8(data.to_vec()) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid UTF-8 content").into_response(),
    };

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let event: nostr::Event = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => {
                errors += 1;
                continue;
            }
        };

        if event.verify().is_err() {
            errors += 1;
            continue;
        }

        match store.save_event(&event) {
            Ok(()) => imported += 1,
            Err(_) => {
                skipped += 1;
            }
        }
    }

    Json(ImportResult {
        imported,
        skipped,
        errors,
    })
    .into_response()
}

// --- WoT Handlers ---

async fn list_wots(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let wots = state.wot_manager.list_wots().await;
    Json(wots).into_response()
}

async fn get_wot(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let wots = state.wot_manager.list_wots().await;
    match wots.into_iter().find(|w| w.id == id) {
        Some(wot) => Json(wot).into_response(),
        None => (StatusCode::NOT_FOUND, "WoT not found").into_response(),
    }
}

#[derive(Deserialize)]
struct CreateWotRequest {
    id: String,
    seed: String,
    #[serde(default = "default_depth")]
    depth: u8,
    #[serde(default = "default_interval")]
    update_interval_hours: u64,
}

fn default_depth() -> u8 { 1 }
fn default_interval() -> u64 { 24 }

async fn create_wot(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let payload: CreateWotRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if let Err(e) = validate_relay_id(&payload.id) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    if payload.depth < 1 || payload.depth > 4 {
        return (StatusCode::BAD_REQUEST, "Depth must be 1-4").into_response();
    }

    // Validate seed is a valid pubkey
    if nostr::PublicKey::parse(&payload.seed).is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid seed pubkey").into_response();
    }

    let wot_config = WotConfig {
        seed: payload.seed,
        depth: payload.depth,
        update_interval_hours: payload.update_interval_hours,
    };

    if let Err(e) = state.wot_manager.add_wot(payload.id.clone(), wot_config.clone()).await {
        return (StatusCode::CONFLICT, e).into_response();
    }

    // Save to config
    let mut config = state.config.write().await;
    config.wots.insert(payload.id.clone(), wot_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::CREATED, "WoT created").into_response()
}

#[derive(Deserialize)]
struct UpdateWotRequest {
    seed: String,
    #[serde(default = "default_depth")]
    depth: u8,
    #[serde(default = "default_interval")]
    update_interval_hours: u64,
}

async fn update_wot(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let payload: UpdateWotRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if payload.depth < 1 || payload.depth > 4 {
        return (StatusCode::BAD_REQUEST, "Depth must be 1-4").into_response();
    }

    if nostr::PublicKey::parse(&payload.seed).is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid seed pubkey").into_response();
    }

    let wot_config = WotConfig {
        seed: payload.seed,
        depth: payload.depth,
        update_interval_hours: payload.update_interval_hours,
    };

    if let Err(e) = state.wot_manager.update_wot(&id, wot_config.clone()).await {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.wots.insert(id, wot_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::OK, "WoT updated").into_response()
}

async fn delete_wot(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    // Check if any relay policies reference this WoT
    let config = state.config.read().await;
    let mut referencing_relays = Vec::new();
    for (relay_id, relay_conf) in &config.relays {
        if relay_conf.policy.write.wot.as_deref() == Some(&id)
            || relay_conf.policy.read.wot.as_deref() == Some(&id)
        {
            referencing_relays.push(relay_id.clone());
        }
    }
    drop(config);

    if !referencing_relays.is_empty() {
        return (
            StatusCode::CONFLICT,
            format!(
                "WoT '{}' is referenced by relay policies: {}. Remove the WoT references first.",
                id,
                referencing_relays.join(", ")
            ),
        )
            .into_response();
    }

    if let Err(e) = state.wot_manager.remove_wot(&id).await {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.wots.remove(&id);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    StatusCode::NO_CONTENT.into_response()
}

// --- Discovery Relay Handlers ---

async fn get_discovery_relays(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let relays = state.wot_manager.get_discovery_relays().await;
    Json(relays).into_response()
}

#[derive(Deserialize)]
struct DiscoveryRelaysPayload {
    relays: Vec<String>,
}

async fn put_discovery_relays(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let payload: DiscoveryRelaysPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    state
        .wot_manager
        .set_discovery_relays(payload.relays.clone())
        .await;

    let mut config = state.config.write().await;
    config.discovery_relays = payload.relays;
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::OK, "Discovery relays updated").into_response()
}

// --- Blossom Handlers ---

#[derive(Serialize)]
struct BlossomResponse {
    id: String,
    #[serde(flatten)]
    config: BlossomConfig,
}

async fn list_blossoms(State(state): State<Arc<GatewayState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let blossoms: Vec<BlossomResponse> = config
        .blossoms
        .iter()
        .map(|(id, cfg)| BlossomResponse {
            id: id.clone(),
            config: cfg.clone(),
        })
        .collect();
    Json(blossoms)
}

async fn get_blossom(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    match config.blossoms.get(&id) {
        Some(cfg) => Json(BlossomResponse {
            id: id.clone(),
            config: cfg.clone(),
        })
        .into_response(),
        None => (StatusCode::NOT_FOUND, "Blossom server not found").into_response(),
    }
}

fn validate_blossom_config(
    config: &BlossomConfig,
    existing_blossoms: &HashMap<String, BlossomConfig>,
    existing_relays: &HashMap<String, RelayConfig>,
    exclude_id: Option<&str>,
) -> Result<(), String> {
    if config.name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if config.subdomain.is_empty() {
        return Err("Subdomain cannot be empty".to_string());
    }
    if config.storage_path.is_empty() {
        return Err("Storage path cannot be empty".to_string());
    }
    // Check subdomain uniqueness across both blossoms and relays
    for (id, existing) in existing_blossoms {
        if Some(id.as_str()) == exclude_id {
            continue;
        }
        if existing.subdomain == config.subdomain {
            return Err(format!(
                "Subdomain '{}' is already used by blossom server '{}'",
                config.subdomain, id
            ));
        }
    }
    for (id, existing) in existing_relays {
        if existing.subdomain == config.subdomain {
            return Err(format!(
                "Subdomain '{}' is already used by relay '{}'",
                config.subdomain, id
            ));
        }
    }
    Ok(())
}

async fn create_blossom(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    #[derive(Deserialize)]
    struct CreateBlossomRequest {
        id: String,
        #[serde(flatten)]
        config: BlossomConfig,
    }

    let payload: CreateBlossomRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if let Err(e) = validate_relay_id(&payload.id) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let mut config = state.config.write().await;

    if config.blossoms.contains_key(&payload.id) {
        return (
            StatusCode::CONFLICT,
            format!("Blossom server '{}' already exists", payload.id),
        )
            .into_response();
    }

    if let Err(e) = validate_blossom_config(&payload.config, &config.blossoms, &config.relays, None) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    config
        .blossoms
        .insert(payload.id.clone(), payload.config.clone());

    if let Err(resp) = save_config(&state, &config).await {
        config.blossoms.remove(&payload.id);
        return resp;
    }

    (
        StatusCode::CREATED,
        Json(BlossomResponse {
            id: payload.id,
            config: payload.config,
        }),
    )
        .into_response()
}

async fn update_blossom(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let new_config: BlossomConfig = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    let mut config = state.config.write().await;

    if !config.blossoms.contains_key(&id) {
        return (StatusCode::NOT_FOUND, "Blossom server not found").into_response();
    }

    if let Err(e) = validate_blossom_config(&new_config, &config.blossoms, &config.relays, Some(&id)) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let old_config = config.blossoms.insert(id.clone(), new_config.clone());

    if let Err(resp) = save_config(&state, &config).await {
        if let Some(old) = old_config {
            config.blossoms.insert(id.clone(), old);
        }
        return resp;
    }

    Json(BlossomResponse {
        id,
        config: new_config,
    })
    .into_response()
}

async fn delete_blossom(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let mut config = state.config.write().await;

    let removed = config.blossoms.remove(&id);
    if removed.is_none() {
        return (StatusCode::NOT_FOUND, "Blossom server not found").into_response();
    }

    if let Err(resp) = save_config(&state, &config).await {
        if let Some(old) = removed {
            config.blossoms.insert(id, old);
        }
        return resp;
    }

    StatusCode::NO_CONTENT.into_response()
}

// --- Blossom Media Handlers (Admin) ---

async fn list_blossom_media(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let store = match state.blossom_stores.get(&id) {
        Some(s) => s.clone(),
        None => return (StatusCode::NOT_FOUND, "Blossom server not found").into_response(),
    };

    match store.list_all() {
        Ok(metas) => {
            let config = state.config.read().await;
            let base_url = match config.blossoms.get(&id) {
                Some(cfg) => {
                    let scheme = if state.domain == "localhost" {
                        "http"
                    } else {
                        "https"
                    };
                    if state.domain == "localhost" {
                        format!("{}://{}.{}:{}", scheme, cfg.subdomain, state.domain, state.port)
                    } else {
                        format!("{}://{}.{}", scheme, cfg.subdomain, state.domain)
                    }
                }
                None => String::new(),
            };
            drop(config);

            let descriptors: Vec<blossom_handlers::BlobDescriptor> = metas
                .iter()
                .map(|m| blossom_handlers::BlobDescriptor::from_meta(m, &base_url))
                .collect();
            Json(descriptors).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Storage error").into_response(),
    }
}

async fn upload_blossom_media(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let store = match state.blossom_stores.get(&id) {
        Some(s) => s.clone(),
        None => return (StatusCode::NOT_FOUND, "Blossom server not found").into_response(),
    };

    let admin_pubkey = {
        let config = state.config.read().await;
        config.admin_pubkey.clone()
    };

    // Parse content type and get filename from Content-Disposition or Content-Type header
    let content_type_header = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if !content_type_header.contains("multipart/form-data") {
        return (StatusCode::BAD_REQUEST, "Expected multipart form data").into_response();
    }

    let mut multipart = match axum::extract::Multipart::from_request(request, &()).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to parse multipart").into_response(),
    };

    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return (StatusCode::BAD_REQUEST, "No file field found").into_response(),
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid multipart data").into_response(),
    };

    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    let file_name = field.file_name().unwrap_or("unknown").to_string();

    let data = match field.bytes().await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to read file data").into_response(),
    };

    // Compute SHA-256
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hasher.finalize();
    let sha256: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

    // Use mime from content type, or guess from filename
    let mime = if content_type == "application/octet-stream" {
        mime_guess::from_path(&file_name)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string()
    } else {
        content_type
    };

    match store.save_blob(&sha256, &data, &mime, &admin_pubkey) {
        Ok(meta) => {
            let config = state.config.read().await;
            let base_url = match config.blossoms.get(&id) {
                Some(cfg) => {
                    let scheme = if state.domain == "localhost" {
                        "http"
                    } else {
                        "https"
                    };
                    if state.domain == "localhost" {
                        format!("{}://{}.{}:{}", scheme, cfg.subdomain, state.domain, state.port)
                    } else {
                        format!("{}://{}.{}", scheme, cfg.subdomain, state.domain)
                    }
                }
                None => String::new(),
            };
            drop(config);

            Json(blossom_handlers::BlobDescriptor::from_meta(&meta, &base_url))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save: {}", e),
        )
            .into_response(),
    }
}

async fn delete_blossom_media(
    State(state): State<Arc<GatewayState>>,
    Path((id, sha256)): Path<(String, String)>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let store = match state.blossom_stores.get(&id) {
        Some(s) => s.clone(),
        None => return (StatusCode::NOT_FOUND, "Blossom server not found").into_response(),
    };

    match store.delete_blob(&sha256) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Blob not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Delete failed").into_response(),
    }
}

// --- Paywall Handlers ---

async fn list_paywalls(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let paywalls = state.paywall_manager.list_paywalls().await;
    Json(paywalls).into_response()
}

async fn get_paywall(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    match state.paywall_manager.get_paywall_info(&id).await {
        Some(info) => Json(info).into_response(),
        None => (StatusCode::NOT_FOUND, "Paywall not found").into_response(),
    }
}

#[derive(Deserialize)]
struct CreatePaywallRequest {
    id: String,
    nwc_string: String,
    price_sats: u64,
    #[serde(default = "default_period")]
    period_days: u32,
}

fn default_period() -> u32 {
    30
}

async fn create_paywall(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let payload: CreatePaywallRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if let Err(e) = validate_relay_id(&payload.id) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    if payload.price_sats == 0 {
        return (StatusCode::BAD_REQUEST, "Price must be greater than 0").into_response();
    }

    let paywall_config = PaywallConfig {
        nwc_string: payload.nwc_string,
        price_sats: payload.price_sats,
        period_days: payload.period_days,
    };

    if let Err(e) = state
        .paywall_manager
        .add_paywall(payload.id.clone(), paywall_config.clone())
        .await
    {
        return (StatusCode::CONFLICT, e).into_response();
    }

    // Save to config
    let mut config = state.config.write().await;
    config.paywalls.insert(payload.id.clone(), paywall_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::CREATED, "Paywall created").into_response()
}

#[derive(Deserialize)]
struct UpdatePaywallRequest {
    nwc_string: String,
    price_sats: u64,
    #[serde(default = "default_period")]
    period_days: u32,
}

async fn update_paywall(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let payload: UpdatePaywallRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if payload.price_sats == 0 {
        return (StatusCode::BAD_REQUEST, "Price must be greater than 0").into_response();
    }

    let paywall_config = PaywallConfig {
        nwc_string: payload.nwc_string,
        price_sats: payload.price_sats,
        period_days: payload.period_days,
    };

    if let Err(e) = state
        .paywall_manager
        .update_paywall(&id, paywall_config.clone())
        .await
    {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.paywalls.insert(id, paywall_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::OK, "Paywall updated").into_response()
}

async fn delete_paywall(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    // Check if any relay policies reference this paywall
    let config = state.config.read().await;
    let mut referencing_relays = Vec::new();
    for (relay_id, relay_conf) in &config.relays {
        if relay_conf.policy.write.paywall.as_deref() == Some(&id)
            || relay_conf.policy.read.paywall.as_deref() == Some(&id)
        {
            referencing_relays.push(relay_id.clone());
        }
    }
    drop(config);

    if !referencing_relays.is_empty() {
        return (
            StatusCode::CONFLICT,
            format!(
                "Paywall '{}' is referenced by relay policies: {}. Remove the paywall references first.",
                id,
                referencing_relays.join(", ")
            ),
        )
            .into_response();
    }

    if let Err(e) = state.paywall_manager.remove_paywall(&id).await {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.paywalls.remove(&id);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct VerifyNwcRequest {
    nwc_string: String,
}

async fn verify_nwc_handler(
    State(state): State<Arc<GatewayState>>,
    Path(_id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid body").into_response(),
    };

    let payload: VerifyNwcRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    match state.paywall_manager.verify_nwc(&payload.nwc_string).await {
        Ok(()) => (StatusCode::OK, "NWC connection verified").into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            format!("NWC verification failed: {}", e),
        )
            .into_response(),
    }
}

async fn get_paywall_whitelist(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    match state.paywall_manager.get_whitelist(&id).await {
        Some(entries) => Json(entries).into_response(),
        None => (StatusCode::NOT_FOUND, "Paywall not found").into_response(),
    }
}

// --- Restart Handler ---

async fn restart_handler(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    tracing::info!("Restart requested via admin UI — exiting process for container restart");

    // Spawn a delayed exit so the HTTP response is sent first
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        std::process::exit(0);
    });

    (StatusCode::OK, "Restarting...").into_response()
}

// --- Update Handlers ---

async fn update_handler(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let manager_secret = match std::env::var("MANAGER_SECRET") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Update service not configured (MANAGER_SECRET not set)",
            )
                .into_response()
        }
    };

    tracing::info!("Update requested via admin UI");

    let client = reqwest::Client::new();
    match client
        .post("http://manager:9090/update")
        .bearer_auth(&manager_secret)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY), body)
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            format!("Failed to reach update service: {}", e),
        )
            .into_response(),
    }
}

async fn update_status_handler(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    // Try reading from shared volume first
    let status_path = std::path::Path::new("/status/update.json");
    if status_path.exists() {
        if let Ok(contents) = tokio::fs::read_to_string(status_path).await {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                contents,
            )
                .into_response();
        }
    }

    // Fallback: proxy to manager service
    let manager_secret = match std::env::var("MANAGER_SECRET") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            return Json(serde_json::json!({"status": "idle"})).into_response();
        }
    };

    let client = reqwest::Client::new();
    match client
        .get("http://manager:9090/status")
        .bearer_auth(&manager_secret)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                body,
            )
                .into_response()
        }
        Err(_) => Json(serde_json::json!({"status": "idle"})).into_response(),
    }
}

// --- Caddy On-Demand TLS ---

async fn caddy_ask_handler(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    let Some(domain) = params.get("domain") else {
        return StatusCode::BAD_REQUEST;
    };

    // Check base domain
    if domain == &state.domain {
        return StatusCode::OK;
    }

    // Check relay/blossom subdomains
    let expected_suffix = format!(".{}", state.domain);
    if domain.ends_with(&expected_suffix) {
        let subdomain = &domain[..domain.len() - expected_suffix.len()];
        let config = state.config.read().await;
        let is_relay = config.relays.values().any(|r| r.subdomain == subdomain);
        let is_blossom = config.blossoms.values().any(|b| b.subdomain == subdomain);
        if is_relay || is_blossom {
            return StatusCode::OK;
        }
    }

    StatusCode::NOT_FOUND
}
