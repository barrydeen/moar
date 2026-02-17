use crate::blossom::auth::{get_x_tag, verify_blossom_auth};
use crate::blossom::store::{BlobMeta, BlobStore};
use crate::config::BlossomConfig;
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct BlossomState {
    pub config: BlossomConfig,
    pub store: Arc<BlobStore>,
    pub server_id: String,
    pub base_url: String,
}

#[derive(Serialize)]
pub struct BlobDescriptor {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub mime_type: String,
    pub uploaded: u64,
}

impl BlobDescriptor {
    pub fn from_meta(meta: &BlobMeta, base_url: &str) -> Self {
        let ext = mime_to_ext(&meta.mime_type);
        let url = if ext.is_empty() {
            format!("{}/{}", base_url, meta.sha256)
        } else {
            format!("{}/{}.{}", base_url, meta.sha256, ext)
        };
        Self {
            url,
            sha256: meta.sha256.clone(),
            size: meta.size,
            mime_type: meta.mime_type.clone(),
            uploaded: meta.uploaded,
        }
    }
}

fn mime_to_ext(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/avif" => "avif",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" => "wav",
        "audio/flac" => "flac",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/html" => "html",
        "application/json" => "json",
        _ => "",
    }
}

pub fn create_blossom_router(state: BlossomState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .expose_headers([
            header::CONTENT_TYPE,
            header::CONTENT_LENGTH,
        ]);

    Router::new()
        .route("/upload", get(head_upload).put(put_upload))
        .route("/list/:pubkey", get(list_blobs))
        .route("/:sha256", get(get_blob).head(head_blob).delete(delete_blob))
        .layer(cors)
        .with_state(Arc::new(state))
}

async fn get_blob(
    State(state): State<Arc<BlossomState>>,
    Path(sha256): Path<String>,
) -> Response {
    // Strip any file extension from the sha256
    let sha256 = sha256.split('.').next().unwrap_or(&sha256);

    let meta = match state.store.get_meta(sha256) {
        Ok(Some(m)) => m,
        Ok(None) => return (StatusCode::NOT_FOUND, "Blob not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Storage error").into_response()
        }
    };

    let blob_path = state.store.get_blob_path(sha256);
    let file = match File::open(&blob_path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "Blob file not found").into_response(),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &meta.mime_type)
        .header(header::CONTENT_LENGTH, meta.size)
        .header(
            header::CACHE_CONTROL,
            "public, max-age=31536000, immutable",
        )
        .body(body)
        .unwrap()
        .into_response()
}

async fn head_blob(
    State(state): State<Arc<BlossomState>>,
    Path(sha256): Path<String>,
) -> Response {
    let sha256 = sha256.split('.').next().unwrap_or(&sha256);

    match state.store.get_meta(sha256) {
        Ok(Some(meta)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, &meta.mime_type)
            .header(header::CONTENT_LENGTH, meta.size)
            .body(Body::empty())
            .unwrap()
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn head_upload(
    State(state): State<Arc<BlossomState>>,
    request: Request<Body>,
) -> Response {
    match verify_blossom_auth(request.headers(), "upload") {
        Ok(event) => {
            let pubkey = event.author().to_hex();
            if !is_upload_allowed(&state.config, &pubkey) {
                return StatusCode::FORBIDDEN.into_response();
            }
            StatusCode::OK.into_response()
        }
        Err(_) => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn put_upload(
    State(state): State<Arc<BlossomState>>,
    request: Request<Body>,
) -> Response {
    let event = match verify_blossom_auth(request.headers(), "upload") {
        Ok(e) => e,
        Err(e) => return (StatusCode::UNAUTHORIZED, e).into_response(),
    };

    let pubkey = event.author().to_hex();
    if !is_upload_allowed(&state.config, &pubkey) {
        return (StatusCode::FORBIDDEN, "Upload not allowed for this pubkey").into_response();
    }

    // Get content type from request
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let max_size = state.config.policy.max_file_size.unwrap_or(100 * 1024 * 1024);

    let body_bytes = match axum::body::to_bytes(request.into_body(), max_size as usize).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "File too large").into_response();
        }
    };

    // Compute SHA-256
    let mut hasher = Sha256::new();
    hasher.update(&body_bytes);
    let hash = hasher.finalize();
    let sha256 = hex::encode(hash);

    // Check if blob already exists
    match state.store.has_blob(&sha256) {
        Ok(true) => {
            if let Ok(Some(meta)) = state.store.get_meta(&sha256) {
                return Json(BlobDescriptor::from_meta(&meta, &state.base_url)).into_response();
            }
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Storage error").into_response();
        }
        _ => {}
    }

    match state
        .store
        .save_blob(&sha256, &body_bytes, &content_type, &pubkey)
    {
        Ok(meta) => (
            StatusCode::OK,
            Json(BlobDescriptor::from_meta(&meta, &state.base_url)),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save blob: {}", e),
        )
            .into_response(),
    }
}

async fn list_blobs(
    State(state): State<Arc<BlossomState>>,
    Path(pubkey): Path<String>,
    request: Request<Body>,
) -> Response {
    if state.config.policy.list.require_auth {
        match verify_blossom_auth(request.headers(), "list") {
            Ok(event) => {
                let auth_pubkey = event.author().to_hex();
                if let Some(allowed) = &state.config.policy.list.allowed_pubkeys {
                    if !allowed.contains(&auth_pubkey) {
                        return (StatusCode::FORBIDDEN, "Not allowed to list").into_response();
                    }
                }
            }
            Err(e) => return (StatusCode::UNAUTHORIZED, e).into_response(),
        }
    }

    match state.store.list_by_pubkey(&pubkey) {
        Ok(metas) => {
            let descriptors: Vec<BlobDescriptor> = metas
                .iter()
                .map(|m| BlobDescriptor::from_meta(m, &state.base_url))
                .collect();
            Json(descriptors).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Storage error").into_response(),
    }
}

async fn delete_blob(
    State(state): State<Arc<BlossomState>>,
    Path(sha256): Path<String>,
    request: Request<Body>,
) -> Response {
    let event = match verify_blossom_auth(request.headers(), "delete") {
        Ok(e) => e,
        Err(e) => return (StatusCode::UNAUTHORIZED, e).into_response(),
    };

    // Verify x tag matches the sha256
    match get_x_tag(&event) {
        Some(x) if x == sha256 => {}
        _ => {
            return (
                StatusCode::FORBIDDEN,
                "Auth event 'x' tag must match the blob sha256",
            )
                .into_response()
        }
    }

    let pubkey = event.author().to_hex();

    // Check if the deleter is the uploader
    match state.store.get_meta(&sha256) {
        Ok(Some(meta)) => {
            if meta.uploader != pubkey {
                return (StatusCode::FORBIDDEN, "Only the uploader can delete").into_response();
            }
        }
        Ok(None) => return (StatusCode::NOT_FOUND, "Blob not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Storage error").into_response()
        }
    }

    match state.store.delete_blob(&sha256) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Blob not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Delete failed").into_response(),
    }
}

fn is_upload_allowed(config: &BlossomConfig, pubkey: &str) -> bool {
    match &config.policy.upload.allowed_pubkeys {
        Some(allowed) => allowed.contains(&pubkey.to_string()),
        None => true,
    }
}

/// Hex encode bytes — using a simple implementation to avoid adding another dep.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}
