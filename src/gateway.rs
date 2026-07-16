use crate::auth::verify_auth_event;
use crate::blossom::handlers::{self as blossom_handlers, BlossomState};
use crate::blossom::store::BlobStore;
use crate::config::{BlossomConfig, CrawlConfig, MoarConfig, PaywallConfig, RelayConfig, SyncConfig, WotConfig};
use crate::crawl::CrawlManager;
use crate::paywall::PaywallManager;
use crate::policy::PolicyEngine;
use crate::server::{self, RelayState};
use crate::stats::{RelayStats, SharedSystemStats, TimeSeriesRing};
use crate::storage::NostrStore;
use crate::sync::SyncManager;
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
use std::net::IpAddr;
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
    pub sync_manager: Arc<SyncManager>,
    pub crawl_manager: Arc<CrawlManager>,
    pub relay_stats: HashMap<String, Arc<RelayStats>>,
    pub time_series: HashMap<String, Arc<RwLock<TimeSeriesRing>>>,
    pub system_stats: SharedSystemStats,
    pub start_time: u64,
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
    relays: HashMap<String, (RelayConfig, Arc<dyn NostrStore>, Arc<PolicyEngine>, Arc<RelayStats>, Arc<RwLock<TimeSeriesRing>>)>,
    blossoms: HashMap<String, (BlossomConfig, Arc<BlobStore>)>,
    config: MoarConfig,
    config_path: PathBuf,
    wot_manager: Arc<WotManager>,
    paywall_manager: Arc<PaywallManager>,
    sync_manager: Arc<SyncManager>,
    crawl_manager: Arc<CrawlManager>,
) -> crate::error::Result<()> {
    let pages_dir = PathBuf::from(&config.pages_dir);
    // Ensure the pages directory exists
    let _ = tokio::fs::create_dir_all(&pages_dir).await;

    let mut router_map = HashMap::new();
    let mut config_map = HashMap::new();
    let mut store_map: HashMap<String, Arc<dyn NostrStore>> = HashMap::new();
    let mut stats_map: HashMap<String, Arc<RelayStats>> = HashMap::new();
    let mut ts_map: HashMap<String, Arc<RwLock<TimeSeriesRing>>> = HashMap::new();
    let mut bg_relay_data = Vec::new();

    for (key, (relay_config, store, policy, stats, ts_ring)) in relays {
        let scheme = if domain == "localhost" { "http" } else { "https" };
        let relay_url = format!(
            "{}://{}.{}",
            scheme, relay_config.subdomain, domain
        );
        store_map.insert(key.clone(), store.clone());
        let db_path = store.db_path().to_string();
        stats_map.insert(key.clone(), stats.clone());
        ts_map.insert(key.clone(), ts_ring.clone());
        bg_relay_data.push((key.clone(), stats.clone(), ts_ring.clone(), store.clone(), db_path));

        // Determine paywall for this relay (write and read reference the same ID)
        let paywall_id = relay_config
            .policy
            .write
            .paywall
            .as_ref()
            .or(relay_config.policy.read.paywall.as_ref())
            .cloned();

        let ip_tracker = Arc::new(crate::rate_limit::IpTracker::new());

        // Spawn periodic cleanup for stale IP tracking entries
        {
            let tracker = ip_tracker.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    tracker.cleanup();
                }
            });
        }

        let has_search = relay_config.search.as_ref().map_or(false, |s| s.enabled);
        let rate_limit_excluded_ips: Vec<IpAddr> = relay_config
            .policy
            .rate_limit
            .excluded_ips
            .iter()
            .filter_map(|s| s.parse::<IpAddr>().ok())
            .collect();
        let rate_limit_excluded_pubkeys = relay_config.policy.rate_limit.excluded_pubkeys.clone();
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
            stats,
            ip_tracker,
            has_search,
            rate_limit_excluded_ips,
            rate_limit_excluded_pubkeys,
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

    let system_stats = crate::stats::SharedSystemStats::default();
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

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
        sync_manager,
        crawl_manager,
        relay_stats: stats_map,
        time_series: ts_map,
        system_stats: system_stats.clone(),
        start_time,
    });

    // Spawn stats background task
    tokio::spawn(crate::stats::stats_background_loop(
        bg_relay_data,
        system_stats,
    ));

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

    if hostname == state.domain || hostname == "localhost" || hostname == &format!("admin.{}", state.domain) {
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
        .route("/api/relays/:id/events/:event_id", delete_route(delete_event_handler))
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
        .route("/api/syncs", get(list_syncs).post(create_sync))
        .route(
            "/api/syncs/:id",
            get(get_sync).put(update_sync).delete(delete_sync),
        )
        .route("/api/syncs/:id/trigger", post(trigger_sync))
        .route("/api/crawls", get(list_crawls).post(create_crawl))
        .route(
            "/api/crawls/:id",
            get(get_crawl).put(update_crawl_handler).delete(delete_crawl),
        )
        .route("/api/crawls/:id/pause", post(pause_crawl))
        .route("/api/crawls/:id/resume", post(resume_crawl))
        .route("/api/og", get(og_proxy_handler))
        .route("/api/stats", get(global_stats_handler))
        .route("/api/stats/:relay_id", get(relay_stats_handler))
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

// --- Delete Event ---

async fn delete_event_handler(
    State(state): State<Arc<GatewayState>>,
    Path((id, event_id)): Path<(String, String)>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let store = match state.relay_stores.get(&id) {
        Some(s) => s.clone(),
        None => return (StatusCode::NOT_FOUND, "Relay not found").into_response(),
    };

    // Parse hex event ID into 32-byte array via nostr::EventId
    let id_bytes: [u8; 32] = match nostr::EventId::from_hex(&event_id) {
        Ok(eid) => *eid.as_bytes(),
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid event ID").into_response(),
    };

    match store.delete_event(&id_bytes) {
        Ok(true) => (StatusCode::OK, "Deleted").into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Event not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to delete event: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Delete failed").into_response()
        }
    }
}

// --- OG Tag Proxy ---

#[derive(Deserialize)]
struct OgQuery {
    url: String,
}

#[derive(Serialize)]
struct OgResult {
    title: Option<String>,
    description: Option<String>,
    image: Option<String>,
}

async fn og_proxy_handler(
    State(state): State<Arc<GatewayState>>,
    Query(query): Query<OgQuery>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    // Basic URL validation
    if !query.url.starts_with("http://") && !query.url.starts_with("https://") {
        return (StatusCode::BAD_REQUEST, Json(OgResult { title: None, description: None, image: None })).into_response();
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .unwrap();

    let html = match client.get(&query.url).header("User-Agent", "Mozilla/5.0 (compatible; MOAR/1.0)").send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => text,
            Err(_) => return Json(OgResult { title: None, description: None, image: None }).into_response(),
        },
        Err(_) => return Json(OgResult { title: None, description: None, image: None }).into_response(),
    };

    fn extract_og_tag(html: &str, property: &str) -> Option<String> {
        for pattern in &[
            format!("property=\"og:{}\"", property),
            format!("property='og:{}'", property),
        ] {
            if let Some(pos) = html.find(pattern.as_str()) {
                let meta_start = html[..pos].rfind("<meta").unwrap_or(pos);
                let meta_end = html[meta_start..].find('>').map(|p| meta_start + p).unwrap_or(html.len());
                let meta_tag = &html[meta_start..meta_end];
                if let Some(content_pos) = meta_tag.find("content=\"").or_else(|| meta_tag.find("content='")) {
                    let quote = meta_tag.as_bytes()[content_pos + 8] as char;
                    let offset = content_pos + 9;
                    if let Some(end) = meta_tag[offset..].find(quote) {
                        return Some(meta_tag[offset..offset + end].to_string());
                    }
                }
            }
        }
        None
    }

    let result = OgResult {
        title: extract_og_tag(&html, "title"),
        description: extract_og_tag(&html, "description"),
        image: extract_og_tag(&html, "image"),
    };

    Json(result).into_response()
}

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

// --- Sync Handlers ---

async fn list_syncs(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let syncs = state.sync_manager.list_syncs().await;
    Json(syncs).into_response()
}

async fn get_sync(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    match state.sync_manager.get_sync(&id).await {
        Some(sync) => Json(sync).into_response(),
        None => (StatusCode::NOT_FOUND, "Sync not found").into_response(),
    }
}

#[derive(Deserialize)]
struct CreateSyncRequest {
    id: String,
    relay: String,
    remote_relays: Vec<String>,
    #[serde(default = "default_sync_interval_api")]
    interval_minutes: u64,
    #[serde(default)]
    authors: Option<Vec<String>>,
    #[serde(default)]
    authors_from_wot: Option<String>,
    #[serde(default)]
    kinds: Option<Vec<u64>>,
    #[serde(default)]
    tags: Option<HashMap<String, Vec<String>>>,
    #[serde(default = "default_sync_limit_api")]
    limit: Option<usize>,
}

fn default_sync_interval_api() -> u64 { 60 }
fn default_sync_limit_api() -> Option<usize> { Some(500) }

async fn create_sync(
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

    let payload: CreateSyncRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if let Err(e) = validate_relay_id(&payload.id) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    // Validate target relay exists
    {
        let config = state.config.read().await;
        if !config.relays.contains_key(&payload.relay) {
            return (
                StatusCode::BAD_REQUEST,
                format!("Target relay '{}' does not exist", payload.relay),
            )
                .into_response();
        }
    }

    if payload.remote_relays.is_empty() {
        return (StatusCode::BAD_REQUEST, "remote_relays cannot be empty").into_response();
    }

    // Validate authors/authors_from_wot mutual exclusivity
    if payload.authors.is_some() && payload.authors_from_wot.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            "Cannot specify both 'authors' and 'authors_from_wot'",
        )
            .into_response();
    }

    // Validate WoT reference if provided
    if let Some(ref wot_id) = payload.authors_from_wot {
        if state.wot_manager.get_set(wot_id).await.is_none() {
            return (
                StatusCode::BAD_REQUEST,
                format!("WoT '{}' does not exist", wot_id),
            )
                .into_response();
        }
    }

    let sync_config = SyncConfig {
        relay: payload.relay,
        remote_relays: payload.remote_relays,
        interval_minutes: payload.interval_minutes,
        authors: payload.authors,
        authors_from_wot: payload.authors_from_wot,
        kinds: payload.kinds,
        tags: payload.tags,
        limit: payload.limit,
    };

    if let Err(e) = state
        .sync_manager
        .add_sync(payload.id.clone(), sync_config.clone())
        .await
    {
        return (StatusCode::CONFLICT, e).into_response();
    }

    let mut config = state.config.write().await;
    config.syncs.insert(payload.id.clone(), sync_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::CREATED, "Sync created").into_response()
}

#[derive(Deserialize)]
struct UpdateSyncRequest {
    relay: String,
    remote_relays: Vec<String>,
    #[serde(default = "default_sync_interval_api")]
    interval_minutes: u64,
    #[serde(default)]
    authors: Option<Vec<String>>,
    #[serde(default)]
    authors_from_wot: Option<String>,
    #[serde(default)]
    kinds: Option<Vec<u64>>,
    #[serde(default)]
    tags: Option<HashMap<String, Vec<String>>>,
    #[serde(default = "default_sync_limit_api")]
    limit: Option<usize>,
}

async fn update_sync(
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

    let payload: UpdateSyncRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    // Validate target relay exists
    {
        let config = state.config.read().await;
        if !config.relays.contains_key(&payload.relay) {
            return (
                StatusCode::BAD_REQUEST,
                format!("Target relay '{}' does not exist", payload.relay),
            )
                .into_response();
        }
    }

    if payload.remote_relays.is_empty() {
        return (StatusCode::BAD_REQUEST, "remote_relays cannot be empty").into_response();
    }

    if payload.authors.is_some() && payload.authors_from_wot.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            "Cannot specify both 'authors' and 'authors_from_wot'",
        )
            .into_response();
    }

    if let Some(ref wot_id) = payload.authors_from_wot {
        if state.wot_manager.get_set(wot_id).await.is_none() {
            return (
                StatusCode::BAD_REQUEST,
                format!("WoT '{}' does not exist", wot_id),
            )
                .into_response();
        }
    }

    let sync_config = SyncConfig {
        relay: payload.relay,
        remote_relays: payload.remote_relays,
        interval_minutes: payload.interval_minutes,
        authors: payload.authors,
        authors_from_wot: payload.authors_from_wot,
        kinds: payload.kinds,
        tags: payload.tags,
        limit: payload.limit,
    };

    if let Err(e) = state
        .sync_manager
        .update_sync(&id, sync_config.clone())
        .await
    {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.syncs.insert(id, sync_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::OK, "Sync updated").into_response()
}

async fn delete_sync(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    if let Err(e) = state.sync_manager.remove_sync(&id).await {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.syncs.remove(&id);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    StatusCode::NO_CONTENT.into_response()
}

async fn trigger_sync(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    match state.sync_manager.trigger_sync(&id).await {
        Ok(()) => (StatusCode::OK, "Sync triggered").into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

// --- Crawl Handlers ---

async fn list_crawls(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let crawls = state.crawl_manager.list_crawls().await;
    Json(crawls).into_response()
}

async fn get_crawl(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    match state.crawl_manager.get_crawl(&id).await {
        Some(crawl) => Json(crawl).into_response(),
        None => (StatusCode::NOT_FOUND, "Crawl not found").into_response(),
    }
}

#[derive(Deserialize)]
struct CreateCrawlRequest {
    id: String,
    relay: String,
    remote_relays: Vec<String>,
    #[serde(default)]
    authors_from_wot: Option<String>,
    #[serde(default)]
    authors: Option<Vec<String>>,
    #[serde(default)]
    kinds: Option<Vec<u64>>,
    #[serde(default)]
    tags: Option<HashMap<String, Vec<String>>>,
    since: u64,
    until: Option<u64>,
    #[serde(default = "default_window_hours_api")]
    window_hours: u64,
    #[serde(default = "default_max_rps_api")]
    max_requests_per_second: u32,
    #[serde(default = "default_batch_size_api")]
    batch_size: usize,
    #[serde(default)]
    paused: bool,
}

fn default_window_hours_api() -> u64 { 24 }
fn default_max_rps_api() -> u32 { 5 }
fn default_batch_size_api() -> usize { 100 }

async fn create_crawl(
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

    let payload: CreateCrawlRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    if let Err(e) = validate_relay_id(&payload.id) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    // Validate target relay exists
    {
        let config = state.config.read().await;
        if !config.relays.contains_key(&payload.relay) {
            return (
                StatusCode::BAD_REQUEST,
                format!("Target relay '{}' does not exist", payload.relay),
            )
                .into_response();
        }
    }

    if payload.remote_relays.is_empty() {
        return (StatusCode::BAD_REQUEST, "remote_relays cannot be empty").into_response();
    }

    if payload.authors.is_some() && payload.authors_from_wot.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            "Cannot specify both 'authors' and 'authors_from_wot'",
        )
            .into_response();
    }

    if let Some(ref wot_id) = payload.authors_from_wot {
        if state.wot_manager.get_set(wot_id).await.is_none() {
            return (
                StatusCode::BAD_REQUEST,
                format!("WoT '{}' does not exist", wot_id),
            )
                .into_response();
        }
    }

    let crawl_config = CrawlConfig {
        relay: payload.relay,
        remote_relays: payload.remote_relays,
        authors_from_wot: payload.authors_from_wot,
        authors: payload.authors,
        kinds: payload.kinds,
        tags: payload.tags,
        since: payload.since,
        until: payload.until,
        window_hours: payload.window_hours,
        max_requests_per_second: payload.max_requests_per_second,
        batch_size: payload.batch_size,
        paused: payload.paused,
    };

    if let Err(e) = state
        .crawl_manager
        .add_crawl(payload.id.clone(), crawl_config.clone())
        .await
    {
        return (StatusCode::CONFLICT, e).into_response();
    }

    let mut config = state.config.write().await;
    config.crawls.insert(payload.id.clone(), crawl_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::CREATED, "Crawl created").into_response()
}

#[derive(Deserialize)]
struct UpdateCrawlRequest {
    relay: String,
    remote_relays: Vec<String>,
    #[serde(default)]
    authors_from_wot: Option<String>,
    #[serde(default)]
    authors: Option<Vec<String>>,
    #[serde(default)]
    kinds: Option<Vec<u64>>,
    #[serde(default)]
    tags: Option<HashMap<String, Vec<String>>>,
    since: u64,
    until: Option<u64>,
    #[serde(default = "default_window_hours_api")]
    window_hours: u64,
    #[serde(default = "default_max_rps_api")]
    max_requests_per_second: u32,
    #[serde(default = "default_batch_size_api")]
    batch_size: usize,
    #[serde(default)]
    paused: bool,
}

async fn update_crawl_handler(
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

    let payload: UpdateCrawlRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response()
        }
    };

    {
        let config = state.config.read().await;
        if !config.relays.contains_key(&payload.relay) {
            return (
                StatusCode::BAD_REQUEST,
                format!("Target relay '{}' does not exist", payload.relay),
            )
                .into_response();
        }
    }

    if payload.remote_relays.is_empty() {
        return (StatusCode::BAD_REQUEST, "remote_relays cannot be empty").into_response();
    }

    if payload.authors.is_some() && payload.authors_from_wot.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            "Cannot specify both 'authors' and 'authors_from_wot'",
        )
            .into_response();
    }

    if let Some(ref wot_id) = payload.authors_from_wot {
        if state.wot_manager.get_set(wot_id).await.is_none() {
            return (
                StatusCode::BAD_REQUEST,
                format!("WoT '{}' does not exist", wot_id),
            )
                .into_response();
        }
    }

    let crawl_config = CrawlConfig {
        relay: payload.relay,
        remote_relays: payload.remote_relays,
        authors_from_wot: payload.authors_from_wot,
        authors: payload.authors,
        kinds: payload.kinds,
        tags: payload.tags,
        since: payload.since,
        until: payload.until,
        window_hours: payload.window_hours,
        max_requests_per_second: payload.max_requests_per_second,
        batch_size: payload.batch_size,
        paused: payload.paused,
    };

    if let Err(e) = state
        .crawl_manager
        .update_crawl(&id, crawl_config.clone())
        .await
    {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.crawls.insert(id, crawl_config);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    (StatusCode::OK, "Crawl updated").into_response()
}

async fn delete_crawl(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    if let Err(e) = state.crawl_manager.remove_crawl(&id).await {
        return (StatusCode::NOT_FOUND, e).into_response();
    }

    let mut config = state.config.write().await;
    config.crawls.remove(&id);
    if let Err(resp) = save_config(&state, &config).await {
        return resp;
    }

    StatusCode::NO_CONTENT.into_response()
}

async fn pause_crawl(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    match state.crawl_manager.pause_crawl(&id).await {
        Ok(()) => (StatusCode::OK, "Crawl paused").into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

async fn resume_crawl(
    State(state): State<Arc<GatewayState>>,
    Path(id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    match state.crawl_manager.resume_crawl(&id).await {
        Ok(()) => (StatusCode::OK, "Crawl resumed").into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
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

// --- Stats Handlers ---

use std::sync::atomic::Ordering::Relaxed;

#[derive(Serialize)]
struct RelayStatsResponse {
    relay_id: String,
    active_connections: i64,
    total_connections: u64,
    events_stored: u64,
    events_saved: u64,
    events_rejected: u64,
    queries_served: u64,
    bytes_rx: u64,
    bytes_tx: u64,
    storage_bytes: u64,
    connections_refused: u64,
    rate_limited_writes: u64,
    rate_limited_reads: u64,
    messages_too_large: u64,
}

fn read_relay_stats(relay_id: &str, stats: &RelayStats) -> RelayStatsResponse {
    RelayStatsResponse {
        relay_id: relay_id.to_string(),
        active_connections: stats.active_connections.load(Relaxed),
        total_connections: stats.total_connections.load(Relaxed),
        events_stored: stats.event_count.load(Relaxed),
        events_saved: stats.events_saved.load(Relaxed),
        events_rejected: stats.events_rejected.load(Relaxed),
        queries_served: stats.queries_served.load(Relaxed),
        bytes_rx: stats.bytes_rx.load(Relaxed),
        bytes_tx: stats.bytes_tx.load(Relaxed),
        storage_bytes: stats.storage_bytes.load(Relaxed),
        connections_refused: stats.connections_refused.load(Relaxed),
        rate_limited_writes: stats.rate_limited_writes.load(Relaxed),
        rate_limited_reads: stats.rate_limited_reads.load(Relaxed),
        messages_too_large: stats.messages_too_large.load(Relaxed),
    }
}

#[derive(Serialize)]
struct GlobalStatsResponse {
    uptime_seconds: u64,
    total_active_connections: i64,
    total_events_stored: u64,
    total_storage_bytes: u64,
    total_bytes_rx: u64,
    total_bytes_tx: u64,
    total_connections_refused: u64,
    total_rate_limited_writes: u64,
    total_rate_limited_reads: u64,
    total_messages_too_large: u64,
    relay_count: usize,
    relays: Vec<RelayStatsResponse>,
    system: crate::stats::SystemStats,
}

async fn global_stats_handler(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut relays = Vec::new();
    let mut total_active: i64 = 0;
    let mut total_events: u64 = 0;
    let mut total_storage: u64 = 0;
    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;
    let mut total_connections_refused: u64 = 0;
    let mut total_rate_limited_writes: u64 = 0;
    let mut total_rate_limited_reads: u64 = 0;
    let mut total_messages_too_large: u64 = 0;

    for (id, stats) in &state.relay_stats {
        let r = read_relay_stats(id, stats);
        total_active += r.active_connections;
        total_events += r.events_stored;
        total_storage += r.storage_bytes;
        total_rx += r.bytes_rx;
        total_tx += r.bytes_tx;
        total_connections_refused += r.connections_refused;
        total_rate_limited_writes += r.rate_limited_writes;
        total_rate_limited_reads += r.rate_limited_reads;
        total_messages_too_large += r.messages_too_large;
        relays.push(r);
    }

    let system = state.system_stats.read().await.clone();

    Json(GlobalStatsResponse {
        uptime_seconds: now - state.start_time,
        total_active_connections: total_active,
        total_events_stored: total_events,
        total_storage_bytes: total_storage,
        total_bytes_rx: total_rx,
        total_bytes_tx: total_tx,
        total_connections_refused,
        total_rate_limited_writes,
        total_rate_limited_reads,
        total_messages_too_large,
        relay_count: relays.len(),
        relays,
        system,
    })
    .into_response()
}

#[derive(Serialize)]
struct RelayStatsDetailResponse {
    #[serde(flatten)]
    stats: RelayStatsResponse,
    history: Vec<crate::stats::TimeBucket>,
}

async fn relay_stats_handler(
    State(state): State<Arc<GatewayState>>,
    Path(relay_id): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    if let Err(resp) = require_auth(request.headers(), &state.sessions).await {
        return resp;
    }

    let stats = match state.relay_stats.get(&relay_id) {
        Some(s) => s,
        None => return (StatusCode::NOT_FOUND, "Relay not found").into_response(),
    };

    let r = read_relay_stats(&relay_id, stats);

    let history = match state.time_series.get(&relay_id) {
        Some(ts) => ts.read().await.entries(),
        None => Vec::new(),
    };

    Json(RelayStatsDetailResponse {
        stats: r,
        history,
    })
    .into_response()
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
