use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use infra::drive_events::DriveEventSource;
use riply_core::{MetadataSearcher, RipBackend};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::metrics::Metrics;
use crate::sse::SseManager;
use app::JobScheduler;

/// Type for an active rip: (cancellation sender, tokio task handle).
type ActiveRip = (watch::Sender<()>, tokio::task::JoinHandle<()>);
use infra::{Config, Database};

use crate::routes::analyze;
use crate::routes::config as cfg;
use crate::routes::files;
use crate::routes::jobs;
use crate::routes::move_ as mover_routes;
use crate::routes::organizer;
use crate::routes::review;
use crate::routes::ripper;
use crate::routes::scan;
use crate::routes::search;
use crate::routes::session_state;
use crate::routes::sse_handler;

/// Stored disc name info (derived or user-set).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscNameInfo {
    /// The original disc name from MakeMKV.
    pub disc_name: String,
    /// Derived name (e.g., "Inception (2007)") or user-set name.
    pub final_name: String,
}

/// Shared application state
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<Config>>,
    pub config_path: Option<String>,
    pub sse: SseManager,
    pub backend: Option<Arc<dyn RipBackend>>,
    pub searcher: Option<Arc<dyn MetadataSearcher>>,
    pub scheduler: JobScheduler,
    /// Active rip tasks keyed by drive index. Stores a cancellation channel
    /// (to signal the rip task to cancel) and the JoinHandle (for aborting
    /// the tokio task).
    pub active_rips: RwLock<HashMap<usize, ActiveRip>>,
    /// Drives with auto-rip enabled (session-only, in-memory).
    pub autorip_drives: RwLock<HashSet<usize>>,
    /// Handle for the autorip background polling task.
    pub autorip_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
    /// Handle for the healthchecks.io heartbeat task.
    pub healthchecks_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
    /// Prometheus metrics collector.
    pub metrics: Metrics,
    /// User-set or derived disc names, keyed by drive index.
    /// Session-only, in-memory.
    pub disc_names: RwLock<HashMap<usize, DiscNameInfo>>,
    /// Drives that have completed rips (MakeMKV may still report "Ripping" state).
    /// Used to override drive state to "Ready" for UI purposes.
    pub completed_rips: RwLock<HashSet<usize>>,
    /// Drives that have already had auto-naming triggered.
    /// Prevents duplicate naming tasks when list_drives() is called repeatedly.
    pub naming_completed: RwLock<HashSet<usize>>,
    /// Auto-naming suggestions per drive: index → (title, year) or None if failed.
    /// Session-only, in-memory.
    pub naming_suggestions: RwLock<HashMap<usize, Option<(String, u16)>>>,
    /// Queued accepts: drives where user accepted naming while rip was in progress.
    /// When rip completes, these are executed automatically.
    pub pending_accepts: RwLock<HashMap<usize, (String, u16)>>,
    /// Cached drives list with timestamp. Invalidation on drive events.
    pub drives_cache: RwLock<Option<(Vec<riply_core::DriveInfo>, std::time::Instant)>>,
    /// Cache TTL for drives list (60s).
    pub drives_cache_ttl: std::time::Duration,
    /// Lock to prevent concurrent detect_drives calls (flatpak is slow).
    pub drives_detect_lock: tokio::sync::Mutex<()>,
    /// Active drive event source (if available). None if no event source detected.
    pub drive_event_source: Option<Arc<dyn DriveEventSource>>,
    /// Receiver for drive events from the listener task.
    pub drive_event_rx: RwLock<Option<mpsc::UnboundedReceiver<infra::drive_events::DriveEvent>>>,
    /// Whether an event source is available (gates autorip toggle).
    pub event_source_available: bool,
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

    pub fn error(msg: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg),
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

/// Detailed health check response
#[derive(serde::Serialize)]
pub struct DetailedHealthResponse {
    pub status: String,
    pub version: String,
    pub database: String,
    pub drives: DriveHealth,
    pub makemkv: MakeMkvHealth,
    pub tmdb: TmdbHealth,
    pub event_source: EventSourceHealth,
}

#[derive(serde::Serialize)]
pub struct DriveHealth {
    pub count: usize,
    pub status: String,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct MakeMkvHealth {
    pub available: bool,
    pub status: String,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct TmdbHealth {
    pub configured: bool,
    pub status: String,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct EventSourceHealth {
    pub available: bool,
    pub status: String,
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

    // Request handlers are all fast (long-running work runs as background
    // jobs streamed over SSE), so a 30s timeout protects against hung
    // handlers. SSE and the static fallback are intentionally outside the
    // timed router: SSE is a long-lived stream.
    //
    // NOTE: The drives endpoint needs a longer timeout (120s) for flatpak
    // MakeMKV latency. We place it in a separate router OUTSIDE the 30s
    // timeout layer so the override takes effect.
    let api = Router::new()
        .merge(files::router(Arc::clone(&state)))
        .merge(review::router(Arc::clone(&state)))
        .merge(scan::router(Arc::clone(&state)))
        .merge(analyze::router(Arc::clone(&state)))
        .merge(mover_routes::router(Arc::clone(&state)))
        .merge(jobs::router(Arc::clone(&state)))
        // ripper routes without drives (drives have separate 120s timeout)
        .merge(ripper::router_no_drives(Arc::clone(&state)))
        .merge(cfg::router(Arc::clone(&state)))
        .merge(search::router(Arc::clone(&state)))
        .merge(organizer::router(Arc::clone(&state)))
        .merge(session_state::router(Arc::clone(&state)))
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)));

    // Drives router with 120s timeout for flatpak MakeMKV latency
    let drives_router = ripper::drives_router(Arc::clone(&state));

    Router::new()
        .merge(drives_router)  // 120s timeout (outer)
        .merge(api)             // 30s timeout (inner)
        .merge(sse_handler::router(Arc::clone(&state)))
        .route("/health", get(health_handler))
        .route(
            "/health/detailed",
            get({
                let state = Arc::clone(&shared_state);
                move || {
                    let state = Arc::clone(&state);
                    async move {
                        let db_status = if state.db.ping().await.is_ok() {
                            "ok".to_string()
                        } else {
                            "error".to_string()
                        };

                        let drives = match state.backend.as_ref().map(|b| b.detect_drives()) {
                            Some(Ok(drives)) => {
                                let drive_count = drives.len();
                                DriveHealth {
                                    count: drive_count,
                                    status: if drive_count > 0 { "ok".to_string() } else { "no_disc".to_string() },
                                    error: None,
                                }
                            }
                            Some(Err(e)) => DriveHealth {
                                count: 0,
                                status: "error".to_string(),
                                error: Some(e.to_string()),
                            },
                            None => DriveHealth {
                                count: 0,
                                status: "unavailable".to_string(),
                                error: Some("No rip backend configured".to_string()),
                            },
                        };

                        let makemkv = match infra::MakemkvBackend::auto_detect() {
                            Ok(_) => MakeMkvHealth {
                                available: true,
                                status: "ok".to_string(),
                                error: None,
                            },
                            Err(e) => MakeMkvHealth {
                                available: false,
                                status: "unavailable".to_string(),
                                error: Some(e.to_string()),
                            },
                        };

                        let config = state.config.read().await;
                        let tmdb_configured = config.tmdb_api_key.as_ref().is_some_and(|k| !k.is_empty());
                        let tmdb = TmdbHealth {
                            configured: tmdb_configured,
                            status: if tmdb_configured { "ok".to_string() } else { "not_configured".to_string() },
                            error: if tmdb_configured { None } else { Some("TMDb API key not configured".to_string()) },
                        };

                        let overall_status = if db_status == "ok" && makemkv.available {
                            "ok".to_string()
                        } else if db_status == "ok" {
                            "degraded".to_string()
                        } else {
                            "error".to_string()
                        };

                        let event_source = EventSourceHealth {
                            available: state.event_source_available,
                            status: if state.event_source_available { "ok".to_string() } else { "unavailable".to_string() },
                            error: if state.event_source_available { None } else { Some("No event source available — polling disabled".to_string()) },
                        };

                        (
                            StatusCode::OK,
                            Json(DetailedHealthResponse {
                                status: overall_status,
                                version: env!("CARGO_PKG_VERSION").to_string(),
                                database: db_status,
                                drives,
                                makemkv,
                                tmdb,
                                event_source,
                            }),
                        )
                    }
                }
            }),
        )
        .route(
            "/ready",
            get({
                let db = shared_state.db.clone();
                move || {
                    let db = db.clone();
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
                }
            }),
        )
        .route(
            "/metrics",
            get(move || {
                let metrics = state.metrics.clone();
                async move { metrics.to_prometheus() }
            }),
        )
        // Static files (embedded frontend)
        .fallback(embedded_assets)
        .layer(TraceLayer::new_for_http())
        // Permissive CORS so the dashboard can be served from any origin
        // (e.g. a dev server or a reverse proxy on a different host).
        .layer(CorsLayer::permissive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request};
    use tower::ServiceExt;

    async fn test_state(dir: &tempfile::TempDir) -> Arc<AppState> {
        let db = Arc::new(
            infra::Database::new(dir.path().join("test.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let state = AppState {
            db,
            config: Arc::new(RwLock::new(infra::Config::default())),
            config_path: None,
            sse: SseManager::new(256),
            backend: None,
            searcher: None,
            scheduler: app::JobScheduler::new(),
            active_rips: RwLock::new(HashMap::new()),
            autorip_drives: RwLock::new(HashSet::new()),
            autorip_handle: RwLock::new(None),
            healthchecks_handle: RwLock::new(None),
            metrics: Metrics::default(),
            disc_names: RwLock::new(HashMap::new()),
            completed_rips: RwLock::new(HashSet::new()),
            naming_completed: RwLock::new(HashSet::new()),
            naming_suggestions: RwLock::new(HashMap::new()),
            pending_accepts: RwLock::new(HashMap::new()),
            drives_cache: RwLock::new(None),
            drives_cache_ttl: std::time::Duration::from_secs(60),
            drives_detect_lock: tokio::sync::Mutex::new(()),
            drive_event_source: None,
            drive_event_rx: RwLock::new(None),
            event_source_available: false,
        };
        Arc::new(state)
    }

    #[tokio::test]
    async fn test_cors_headers_present() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(&dir).await;
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .header("origin", "http://localhost:5173")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&header::HeaderValue::from_static("*"))
        );
    }

    #[tokio::test]
    async fn test_sse_route_not_timed_out() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(&dir).await;
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/sse")
                    .header("accept", "text/event-stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // The SSE endpoint returns a 200 and stays open; the timeout layer
        // must not apply to it, otherwise the stream is cut after 30s.
        assert_eq!(response.status(), StatusCode::OK);
    }
}
