use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use tower_http::services::{ServeDir, ServeFile};
use riply_core::{DriveInfo, RipBackend, RipEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use crate::sse::SseManager;
use app::JobScheduler;
use infra::{Config, Database};

/// Path to SvelteKit build directory (relative to binary, or absolute)
fn svelte_build_dir() -> std::path::PathBuf {
    // Try relative to the workspace root first (development)
    // Then try relative to the binary (production)
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
}

/// Create the API router with all endpoints and static file serving
pub fn create_router(state: Arc<AppState>) -> Router {
    info!("Creating API router");

    let build_dir = svelte_build_dir();
    info!("Serving SvelteKit from: {:?}", build_dir);

    let spa_fallback = ServeFile::new(build_dir.join("index.html"));

    let static_service = ServeDir::new(&build_dir)
        .not_found_service(spa_fallback);

    Router::new()
        // API routes
        .route("/health", get(health_check))
        .route("/api/files", get(list_files))
        .route(
            "/api/files/:id",
            get(get_file).post(update_file).delete(delete_file),
        )
        .route("/api/review", get(get_review_queue))
        .route("/api/review/:id/accept", post(accept_file))
        .route("/api/review/:id/reject", post(reject_file))
        .route("/api/scan", post(start_scan))
        .route("/api/analyze", post(start_analyze))
        .route("/api/move", post(start_move))
        .route("/api/move/status", get(get_move_status))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/:id", get(get_job))
        .route("/api/jobs/:id/cancel", post(cancel_job))
        .route("/api/ripper/drives", get(list_drives))
        .route("/api/ripper/:index/start", post(start_rip))
        .route("/api/ripper/:index/cancel", post(cancel_rip))
        .route("/api/config", get(get_config).put(update_config))
        .route("/api/sse", get(sse_endpoint))
        // Serve SvelteKit build output, with SPA fallback for client-side routing
        .fallback_service(static_service)
        .with_state(state)
}

// --- Pagination ---

#[derive(Debug, Deserialize)]
pub struct FileListParams {
    pub status: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

// --- Request / Response types ---

#[derive(Debug, Deserialize)]
pub struct UpdateFileBody {
    pub canonical_title: Option<String>,
    pub canonical_year: Option<u32>,
    pub series_name: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub media_type: Option<String>,
}

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

// --- Health ---

async fn health_check() -> &'static str {
    "OK"
}

// --- Files ---

async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileListParams>,
) -> Result<Json<ApiResponse<PaginatedResponse<riply_core::FileRecord>>>, (StatusCode, String)> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(200);

    let files = if let Some(ref status) = params.status {
        state
            .db
            .get_files_by_status(status)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        state
            .db
            .get_all_files()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let total = files.len() as i64;
    let start = ((page - 1) * per_page) as usize;
    let items: Vec<_> = files
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .collect();

    Ok(Json(ApiResponse::ok(PaginatedResponse {
        items,
        total,
        page,
        per_page,
    })))
}

async fn get_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<riply_core::FileRecord>>, (StatusCode, String)> {
    match state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        Some(file) => Ok(Json(ApiResponse::ok(file))),
        None => Err((StatusCode::NOT_FOUND, format!("File not found: {}", id))),
    }
}

async fn update_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateFileBody>,
) -> Result<Json<ApiResponse<riply_core::FileRecord>>, (StatusCode, String)> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("File not found: {}", id)))?;

    if let Some(title) = body.canonical_title {
        file.canonical_title = Some(title);
    }
    if let Some(year) = body.canonical_year {
        file.canonical_year = Some(year);
    }
    if let Some(name) = body.series_name {
        file.series_name = Some(name);
    }
    if let Some(season) = body.season_number {
        file.season_number = Some(season);
    }
    if let Some(episode) = body.episode_number {
        file.episode_number = Some(episode);
    }
    if let Some(mt) = body.media_type {
        file.media_type = match mt.as_str() {
            "tv" | "Tv" | "TV" => riply_core::MediaType::Tv,
            _ => riply_core::MediaType::Movie,
        };
    }

    state
        .db
        .update_file(&file)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(file)))
}

async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    if state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_none()
    {
        return Err((StatusCode::NOT_FOUND, format!("File not found: {}", id)));
    }

    state
        .db
        .delete_file(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(id)))
}

// --- Review ---

async fn get_review_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<riply_core::FileRecord>>>, (StatusCode, String)> {
    let files = state
        .db
        .get_files_by_status("Needs Review")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(files)))
}

async fn accept_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<riply_core::FileRecord>>, (StatusCode, String)> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("File not found: {}", id)))?;

    // Validate pipeline transition: only Matched or NeedsReview can be accepted
    let valid = matches!(
        file.status,
        riply_core::PipelineStatus::Matched | riply_core::PipelineStatus::NeedsReview
    );
    if !valid {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Cannot accept file in '{}' status (must be Matched or NeedsReview)",
                file.status
            ),
        ));
    }

    file.update_status(riply_core::PipelineStatus::Confirmed);

    state
        .db
        .update_file(&file)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.sse.send_status(&format!("review:accepted:{}", id));

    Ok(Json(ApiResponse::ok(file)))
}

async fn reject_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<riply_core::FileRecord>>, (StatusCode, String)> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("File not found: {}", id)))?;

    // Validate pipeline transition: only NeedsReview can be rejected
    if file.status != riply_core::PipelineStatus::NeedsReview {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Cannot reject file in '{}' status (must be NeedsReview)",
                file.status
            ),
        ));
    }

    file.update_status(riply_core::PipelineStatus::Failed);

    state
        .db
        .update_file(&file)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.sse.send_status(&format!("review:rejected:{}", id));

    Ok(Json(ApiResponse::ok(file)))
}

// --- Move ---

#[derive(Debug, Deserialize)]
pub struct StartMoveBody {
    pub movies_dir: Option<String>,
    pub tv_dir: Option<String>,
}

async fn start_move(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartMoveBody>,
) -> Result<Json<ApiResponse<JobStartedResponse>>, (StatusCode, String)> {
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        let m = body
            .movies_dir
            .clone()
            .unwrap_or_else(|| config.movies_directory.clone());
        let t = body
            .tv_dir
            .clone()
            .unwrap_or_else(|| config.tv_directory.clone());
        (m, t)
    };

    let db = state.db.clone();
    let sse = state.sse.clone();
    let m = movies_dir.clone();
    let t = tv_dir.clone();

    let job_id = state
        .scheduler
        .start_move_job(&movies_dir, &tv_dir, move |cancel, progress| {
            let db = db.clone();
            let sse = sse.clone();
            let movies_path = std::path::PathBuf::from(&m);
            let tv_path = std::path::PathBuf::from(&t);
            async move {
                let files = db.get_files_by_status("Confirmed").await?;
                if files.is_empty() {
                    sse.send_status("move:complete:moved=0,errors=0");
                    return Ok(());
                }

                progress.set_total(files.len());
                sse.send_status(&format!("move:starting:{}", files.len()));

                let mover = app::MoverService::new();
                let mut moved = 0;
                let mut errors = 0;

                for (i, file) in files.iter().enumerate() {
                    if cancel.is_cancelled() {
                        sse.send_status("move:cancelled");
                        return Ok(());
                    }
                    match mover.move_file(file, &movies_path, &tv_path).await {
                        Ok(updated) => {
                            if let Err(e) = db.update_file(&updated).await {
                                sse.send_log(
                                    "warn",
                                    &format!("Failed to update {}: {}", file.filename, e),
                                );
                                errors += 1;
                            } else {
                                moved += 1;
                            }
                        }
                        Err(e) => {
                            sse.send_log(
                                "warn",
                                &format!("Failed to move {}: {}", file.filename, e),
                            );
                            errors += 1;
                        }
                    }
                    progress.increment();
                    if (i + 1) % 10 == 0 || i + 1 == files.len() {
                        sse.send_status(&format!("move:progress:{}:{}", i + 1, files.len()));
                    }
                }

                sse.send_status(&format!("move:complete:moved={},errors={}", moved, errors));
                info!("Move complete: {} moved, {} errors", moved, errors);
                Ok(())
            }
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(JobStartedResponse {
        job_id,
        status: "Queued".to_string(),
    })))
}

#[derive(Serialize)]
pub struct MoveStatusResponse {
    pub confirmed_count: i64,
    pub moved_count: i64,
}

async fn get_move_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<MoveStatusResponse>>, (StatusCode, String)> {
    let confirmed = state
        .db
        .count_files_by_status("Confirmed")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let moved = state
        .db
        .count_files_by_status("Moved")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(MoveStatusResponse {
        confirmed_count: confirmed,
        moved_count: moved,
    })))
}

// --- Scan ---

#[derive(Debug, Deserialize)]
pub struct ScanBody {
    pub directory: String,
}

#[derive(Serialize)]
pub struct JobStartedResponse {
    pub job_id: String,
    pub status: String,
}

async fn start_scan(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ScanBody>,
) -> Result<Json<ApiResponse<JobStartedResponse>>, (StatusCode, String)> {
    let dir = PathBuf::from(&body.directory);
    if !dir.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Directory does not exist: {}", body.directory),
        ));
    }

    let db = state.db.clone();
    let sse = state.sse.clone();
    let directory = body.directory.clone();

    let job_id = state
        .scheduler
        .start_scan_job(&body.directory, move |cancel, progress| {
            let db = db.clone();
            let sse = sse.clone();
            let directory = directory.clone();
            async move {
                let scanner = app::ScannerService::new();
                let records = scanner.scan_directory(&dir).await?;
                progress.set_total(records.len());

                sse.send_status(&format!("scan:starting:{}", records.len()));

                for (i, record) in records.iter().enumerate() {
                    if cancel.is_cancelled() {
                        sse.send_status("scan:cancelled");
                        return Ok(());
                    }
                    if let Err(e) = db.save_file(record).await {
                        sse.send_log(
                            "warn",
                            &format!("Failed to save {}: {}", record.filename, e),
                        );
                    }
                    progress.increment();
                    if (i + 1) % 10 == 0 || i + 1 == records.len() {
                        sse.send_status(&format!("scan:progress:{}:{}", i + 1, records.len()));
                    }
                }

                sse.send_status(&format!("scan:complete:{}", records.len()));
                info!("Scan complete: {} files from {}", records.len(), directory);
                Ok(())
            }
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(JobStartedResponse {
        job_id,
        status: "Queued".to_string(),
    })))
}

// --- Analyze ---

async fn start_analyze(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<JobStartedResponse>>, (StatusCode, String)> {
    let db = state.db.clone();
    let sse = state.sse.clone();
    let config = state.config.read().await.clone();

    let job_id = state
        .scheduler
        .start_analyze_job(move |cancel, progress| {
            let db = db.clone();
            let sse = sse.clone();
            async move {
                let records = db.get_files_by_status("Scanned").await?;
                if records.is_empty() {
                    sse.send_status("analyze:complete:0");
                    return Ok(());
                }

                progress.set_total(records.len());
                sse.send_status(&format!("analyze:starting:{}", records.len()));

                let searcher: Option<Box<dyn riply_core::MetadataSearcher>> =
                    if config.tmdb_api_key.is_some() || config.omdb_api_key.is_some() {
                        let client = infra::HttpClient::new(
                            config.tmdb_api_key.clone(),
                            config.omdb_api_key.clone(),
                        );
                        Some(Box::new(client))
                    } else {
                        None
                    };

                let analyzer = app::AnalyzerService::new(searcher);
                let mut matched = 0;
                let mut review = 0;

                for (i, record) in records.iter().enumerate() {
                    if cancel.is_cancelled() {
                        sse.send_status("analyze:cancelled");
                        return Ok(());
                    }
                    match analyzer.analyze_file(record).await {
                        Ok(updated) => {
                            match updated.status {
                                riply_core::PipelineStatus::Matched => matched += 1,
                                riply_core::PipelineStatus::NeedsReview => review += 1,
                                _ => {}
                            }
                            let _ = db.update_file(&updated).await;
                        }
                        Err(e) => {
                            sse.send_log(
                                "warn",
                                &format!("Failed to analyze {}: {}", record.filename, e),
                            );
                        }
                    }
                    progress.increment();
                    if (i + 1) % 10 == 0 || i + 1 == records.len() {
                        sse.send_status(&format!("analyze:progress:{}:{}", i + 1, records.len()));
                    }
                }

                sse.send_status(&format!(
                    "analyze:complete:matched={},review={}",
                    matched, review
                ));
                info!(
                    "Analysis complete: {} matched, {} need review",
                    matched, review
                );
                Ok(())
            }
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(JobStartedResponse {
        job_id,
        status: "Queued".to_string(),
    })))
}

// --- Jobs ---

#[derive(Serialize)]
pub struct JobInfoResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub progress_total: usize,
    pub progress_processed: usize,
}

async fn list_jobs(State(state): State<Arc<AppState>>) -> Json<ApiResponse<Vec<JobInfoResponse>>> {
    let jobs = state.scheduler.list_jobs().await;
    let items: Vec<JobInfoResponse> = jobs
        .into_iter()
        .map(|j| JobInfoResponse {
            id: j.id,
            kind: j.kind.to_string(),
            status: j.status.to_string(),
            progress_total: j.progress.total,
            progress_processed: j.progress.processed,
        })
        .collect();
    Json(ApiResponse::ok(items))
}

async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<JobInfoResponse>>, (StatusCode, String)> {
    let job = state
        .scheduler
        .get_job_status(&id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(Json(ApiResponse::ok(JobInfoResponse {
        id: job.id,
        kind: job.kind.to_string(),
        status: job.status.to_string(),
        progress_total: job.progress.total,
        progress_processed: job.progress.processed,
    })))
}

async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    state
        .scheduler
        .cancel_job(&id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    state.sse.send_status(&format!("job:cancelled:{}", id));
    Ok(Json(ApiResponse::ok(id)))
}

// --- Ripper ---

async fn list_drives(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<DriveInfo>>>, (StatusCode, String)> {
    let backend = match &state.backend {
        Some(b) => b,
        None => return Ok(Json(ApiResponse::ok(vec![]))),
    };

    match backend.detect_drives() {
        Ok(drives) => Ok(Json(ApiResponse::ok(drives))),
        Err(e) => {
            info!("Failed to detect drives: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to detect drives: {}", e),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StartRipBody {
    pub destination: Option<String>,
}

async fn start_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    Json(body): Json<StartRipBody>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    let backend = state.backend.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "No ripping backend available. Install makemkvcon or Flatpak package.".to_string(),
    ))?;

    let dest = match body.destination {
        Some(d) => d,
        None => {
            let config = state.config.read().await;
            config
                .scan_directory
                .clone()
                .unwrap_or_else(|| ".".to_string())
        }
    };

    let mut handle = backend
        .start_rip(index, &dest)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Spawn a task that runs the rip and broadcasts progress via SSE
    let sse = state.sse.clone();
    let rip_handle = tokio::spawn(async move {
        sse.send_status(&format!("rip:starting:{}", index));

        while let Some(event) = handle.next_event().await {
            match event {
                RipEvent::Progress {
                    current,
                    total,
                    max,
                } => {
                    let pct = if total > 0 && max > 0 {
                        (current as f64 / total as f64 * 100.0) as u32
                    } else {
                        0
                    };
                    sse.send_status(&format!("rip:progress:{}:{}", index, pct));
                }
                RipEvent::Complete => {
                    sse.send_status(&format!("rip:complete:{}", index));
                    break;
                }
                RipEvent::Error(e) => {
                    sse.send_status(&format!("rip:error:{}:{}", index, e));
                    break;
                }
                RipEvent::Message(msg) => {
                    sse.send_status(&format!("rip:message:{}:{}", index, msg));
                }
            }
        }
    });

    // Store the handle so cancel_rip can abort it
    state.active_rips.write().await.insert(index, rip_handle);

    Ok(Json(ApiResponse::ok(format!(
        "Rip started on drive {}",
        index
    ))))
}

async fn cancel_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<String>> {
    let mut rips = state.active_rips.write().await;
    if let Some(handle) = rips.remove(&index) {
        handle.abort();
        state.sse.send_status(&format!("rip:cancelled:{}", index));
        info!("Cancelled rip on drive {}", index);
        Json(ApiResponse::ok(format!("Drive {} rip cancelled", index)))
    } else {
        info!("No active rip found for drive {}", index);
        Json(ApiResponse::ok(format!("No active rip on drive {}", index)))
    }
}

// --- Config ---

async fn get_config(State(state): State<Arc<AppState>>) -> Json<ApiResponse<Config>> {
    let config = state.config.read().await;
    Json(ApiResponse::ok(config.clone()))
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdateBody {
    pub database_path: Option<String>,
    pub scan_directory: Option<String>,
    pub movies_directory: Option<String>,
    pub tv_directory: Option<String>,
    pub verbose: Option<bool>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub tmdb_api_key: Option<String>,
    pub omdb_api_key: Option<String>,
    pub ripping_command: Option<String>,
}

async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConfigUpdateBody>,
) -> Result<Json<ApiResponse<Config>>, (StatusCode, String)> {
    let mut config = state.config.write().await;

    if let Some(v) = body.database_path {
        config.database_path = v;
    }
    if let Some(v) = body.scan_directory {
        config.scan_directory = Some(v);
    }
    if let Some(v) = body.movies_directory {
        config.movies_directory = v;
    }
    if let Some(v) = body.tv_directory {
        config.tv_directory = v;
    }
    if let Some(v) = body.verbose {
        config.verbose = v;
    }
    if let Some(v) = body.host {
        config.host = v;
    }
    if let Some(v) = body.port {
        config.port = v;
    }
    if let Some(v) = body.tmdb_api_key {
        config.tmdb_api_key = Some(v);
    }
    if let Some(v) = body.omdb_api_key {
        config.omdb_api_key = Some(v);
    }
    if let Some(v) = body.ripping_command {
        config.ripping.command = Some(v);
    }

    let updated = config.clone();

    // Persist to disk if config path is known
    if let Some(ref path) = state.config_path.clone() {
        if let Err(e) = updated.save(path) {
            tracing::warn!("Failed to save config to {}: {}", path, e);
        }
    }

    Ok(Json(ApiResponse::ok(updated)))
}

// --- SSE ---

async fn sse_endpoint(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    info!("SSE endpoint connected");

    let mut rx = state.sse.subscribe();

    let stream = async_stream::stream! {
        yield Ok(Event::default().event("connected").data("SSE connection established"));
        while let Ok(msg) = rx.recv().await {
            yield Ok(SseManager::message_to_event(&msg));
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}
