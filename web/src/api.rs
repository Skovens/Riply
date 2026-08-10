use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    extract::State,
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use riply_core::{MetadataSearcher, RipBackend};
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::metrics::Metrics;
use crate::sse::SseManager;
use app::JobScheduler;
use infra::{Config, Database};

use crate::routes::analyze;
use crate::routes::config as cfg;
use crate::routes::files;
use crate::routes::jobs;
use crate::routes::move_ as mover_routes;
use crate::routes::review;
use crate::routes::ripper;
use crate::routes::scan;
use crate::routes::search;
use crate::routes::sse_handler;

/// Shared application state
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<Config>>,
    pub config_path: Option<String>,
    pub sse: SseManager,
    pub backend: Option<Arc<dyn RipBackend>>,
    pub searcher: Option<Arc<dyn MetadataSearcher>>,
    pub scheduler: JobScheduler,
    /// Active rip tasks keyed by drive index. Used for cancellation.
    pub active_rips: RwLock<HashMap<usize, tokio::task::JoinHandle<()>>>,
    /// Drives with auto-rip enabled (session-only, in-memory).
    pub autorip_drives: RwLock<HashSet<usize>>,
    /// Handle for the autorip background polling task.
    pub autorip_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
    /// Handle for the healthchecks.io heartbeat task.
    pub healthchecks_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
    /// Prometheus metrics collector.
    pub metrics: Metrics,
}

#[derive(Debug, serde::Deserialize)]
pub struct FileListParams {
    pub status: Option<String>,
    pub search: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(serde::Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(serde::Serialize)]
pub struct ApiResponse<T: serde::Serialize> {
    pub success: bool,
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: serde::Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

#[derive(serde::Serialize)]
pub struct JobStartedResponse {
    pub job_id: String,
    pub status: String,
}

/// The compiled SvelteKit frontend, embedded into the binary at build time.
/// Build it with `npm run build` in `web/svelte` (see `build.rs`).
#[derive(rust_embed::RustEmbed)]
#[folder = "svelte/build"]
struct SvelteAssets;

const IMMUTABLE_PREFIX: &str = "_app/";

async fn embedded_assets(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let asset = SvelteAssets::get(path).or_else(|| SvelteAssets::get("index.html"));
    let Some(asset) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let cache = if path == "index.html" || !path.starts_with(IMMUTABLE_PREFIX) {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };
    Response::builder()
        .header("Content-Type", asset.metadata.mimetype())
        .header("Cache-Control", cache)
        .body(axum::body::Body::from(asset.data.into_owned()))
        .unwrap()
}

/// Health check response
#[derive(serde::Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Readiness check response
#[derive(serde::Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub database: String,
    pub error: Option<String>,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Create the API router with all endpoints and static file serving
pub fn create_router(state: Arc<AppState>) -> Router {
    info!("Creating API router");

    info!("Serving embedded frontend");

    let shared_state = Arc::clone(&state);

    Router::new()
        .merge(files::router(Arc::clone(&state)))
        .merge(review::router(Arc::clone(&state)))
        .merge(scan::router(Arc::clone(&state)))
        .merge(analyze::router(Arc::clone(&state)))
        .merge(mover_routes::router(Arc::clone(&state)))
        .merge(jobs::router(Arc::clone(&state)))
        .merge(ripper::router(Arc::clone(&state)))
        .merge(cfg::router(Arc::clone(&state)))
        .merge(search::router(Arc::clone(&state)))
        .merge(sse_handler::router(Arc::clone(&state)))
        .route("/health", get(health_handler))
        .route("/ready", get(move || {
            let db = shared_state.db.clone();
            async move {
                let db_ok = db.ping().await.is_ok();
                if db_ok {
                    (
                        StatusCode::OK,
                        Json(ReadinessResponse {
                            status: "ready".to_string(),
                            database: "ok".to_string(),
                            error: None,
                        }),
                    )
                } else {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(ReadinessResponse {
                            status: "unavailable".to_string(),
                            database: "error".to_string(),
                            error: Some("Database unavailable".to_string()),
                        }),
                    )
                }
            }
        }))
        .route("/metrics", get(move || {
            let metrics = state.metrics.clone();
            async move { metrics.to_prometheus() }
        }))
        // Static files (embedded frontend)
        .fallback(embedded_assets)
        .layer(TraceLayer::new_for_http())
}
