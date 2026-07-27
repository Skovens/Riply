use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use riply_core::RipBackend;
use tokio::sync::RwLock;
use tower_http::services::{ServeDir, ServeFile};
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
use crate::routes::sse_handler;

/// Shared application state
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<Config>>,
    pub config_path: Option<String>,
    pub sse: SseManager,
    pub backend: Option<Arc<dyn RipBackend>>,
    pub scheduler: JobScheduler,
    /// Active rip tasks keyed by drive index. Used for cancellation.
    pub active_rips: RwLock<HashMap<usize, tokio::task::JoinHandle<()>>>,
    /// Handle for the autorip background polling task.
    pub autorip_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
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

fn svelte_build_dir() -> std::path::PathBuf {
    let candidates = [
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("svelte/build"),
        std::path::PathBuf::from("web/svelte/build"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    candidates[0].clone()
}

/// Create the API router with all endpoints and static file serving
pub fn create_router(state: Arc<AppState>) -> Router {
    info!("Creating API router");

    let build_dir = svelte_build_dir();
    info!("Serving SvelteKit from: {:?}", build_dir);

    let spa_fallback = ServeFile::new(build_dir.join("index.html"));
    let static_service = ServeDir::new(&build_dir).not_found_service(spa_fallback);

    Router::new()
        .route("/health", get(|| async { "OK" }))
        // Files
        .route("/api/files", get(files::list_files))
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
        .route("/api/ripper/autorip/toggle", post(ripper::autorip_toggle))
        // Config
        .route("/api/config", get(cfg::get_config).put(cfg::update_config))
        // SSE
        .route("/api/sse", get(sse_handler::sse_endpoint))
        // Static files
        .fallback_service(static_service)
        .with_state(state)
}
