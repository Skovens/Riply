use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use riply_core::{MetadataSearcher, RipBackend};
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::info;

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

/// Create the API router with all endpoints and static file serving
pub fn create_router(state: Arc<AppState>) -> Router {
    info!("Creating API router");

    info!("Serving embedded frontend");

    Router::new()
        .route("/health", get(|| async { "OK" }))
        // Files
        .route("/api/files", get(files::list_files).delete(files::reset_db))
        .route(
            "/api/files/:id",
            get(files::get_file)
                .post(files::update_file)
                .delete(files::delete_file),
        )
        // Review
        .route("/api/review", get(review::get_review_queue))
        .route("/api/review/:id/accept", post(review::accept_file))
        .route("/api/review/:id/reject", post(review::reject_file))
        // Scan
        .route("/api/scan", post(scan::start_scan))
        // Analyze
        .route("/api/analyze", post(analyze::start_analyze))
        .route("/api/analyze/retry-failed", post(analyze::retry_failed))
        // Move
        .route("/api/move", post(mover_routes::start_move))
        .route("/api/move/status", get(mover_routes::get_move_status))
        .route("/api/move/preview", get(mover_routes::preview_move))
        // Jobs
        .route("/api/jobs", get(jobs::list_jobs))
        .route("/api/jobs/:id", get(jobs::get_job))
        .route("/api/jobs/:id/cancel", post(jobs::cancel_job))
        // Ripper
        .route("/api/ripper/drives", get(ripper::list_drives))
        .route("/api/ripper/status", get(ripper::ripper_status))
        .route("/api/ripper/:index/start", post(ripper::start_rip))
        .route("/api/ripper/:index/cancel", post(ripper::cancel_rip))
        .route("/api/ripper/:index/eject", post(ripper::eject_disc))
        .route("/api/ripper/autorip", get(ripper::autorip_status))
        .route(
            "/api/ripper/autorip/:index/toggle",
            post(ripper::autorip_toggle),
        )
        // Config
        .route("/api/config", get(cfg::get_config).put(cfg::update_config))
        .route("/api/config/reset", post(cfg::reset_config))
        // Search
        .route("/api/search", get(search::search_metadata))
        // SSE
        .route("/api/sse", get(sse_handler::sse_endpoint))
        // Static files (embedded frontend)
        .fallback(embedded_assets)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
