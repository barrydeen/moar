use crate::auth::verify_auth_event;
use crate::config::{MoarConfig, RelayConfig};
use crate::policy::PolicyEngine;
use crate::server::{self, RelayState};
use crate::storage::NostrStore;
use axum::{
    body::Body,
    extract::{Host, Path, Request, State},
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
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
    pub config: Arc<RwLock<MoarConfig>>,
    pub config_path: PathBuf,
    pub pending_restart: Arc<RwLock<bool>>,
    pub sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
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
    config: MoarConfig,
    config_path: PathBuf,
) -> crate::error::Result<()> {
    let mut router_map = HashMap::new();
    let mut config_map = HashMap::new();

    for (_key, (config, store, policy)) in relays {
        let state = Arc::new(RelayState::new(config.clone(), store, policy));
        let app = server::create_relay_router(state);
        router_map.insert(config.subdomain.clone(), app);
        config_map.insert(config.subdomain.clone(), config);
    }

    let state = Arc::new(GatewayState {
        domain: domain.clone(),
        port,
        relay_routers: router_map,
        relay_configs: config_map,
        config: Arc::new(RwLock::new(config)),
        config_path,
        pending_restart: Arc::new(RwLock::new(false)),
        sessions: Arc::new(RwLock::new(HashMap::new())),
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
    }

    (
        StatusCode::NOT_FOUND,
        format!("Relay not found for host: {}", hostname),
    )
        .into_response()
}

// --- Admin Router ---

pub fn admin_router() -> Router<Arc<GatewayState>> {
    Router::new()
        .route("/", get(serve_index))
        .route("/admin", get(serve_admin))
        .route("/api/login", post(login_handler))
        .route("/api/logout", post(logout_handler))
        .route("/api/status", get(status_handler))
        .route("/api/relays", get(list_relays).post(create_relay))
        .route(
            "/api/relays/:id",
            get(get_relay).put(update_relay).delete(delete_relay),
        )
}

async fn serve_index() -> impl IntoResponse {
    Html(include_str!("web/index.html"))
}

async fn serve_admin() -> impl IntoResponse {
    Html(include_str!("web/admin.html"))
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

    let token = uuid::Uuid::new_v4().to_string();
    let pubkey = event.author().to_hex();

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
    exclude_id: Option<&str>,
) -> Result<(), String> {
    if config.name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if config.subdomain.is_empty() {
        return Err("Subdomain cannot be empty".to_string());
    }
    // Check subdomain uniqueness
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

    if let Err(e) = validate_relay_config(&payload.config, &config.relays, None) {
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

    if let Err(e) = validate_relay_config(&new_config, &config.relays, Some(&id)) {
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

    StatusCode::NO_CONTENT.into_response()
}
