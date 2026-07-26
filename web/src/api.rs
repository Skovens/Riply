use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use crate::sse::SseManager;
use infra::{Config, Database};

/// Shared application state
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<Config>>,
    pub sse: SseManager,
    pub mkv_client: Option<Arc<infra::MakeMkvClient>>,
}

/// Create the API router with all endpoints
pub fn create_router(state: Arc<AppState>) -> Router {
    info!("Creating API router");

    Router::new()
        .route("/health", get(health_check))
        .route("/api/files", get(list_files))
        .route(
            "/api/files/:id",
            get(get_file).post(update_file).delete(delete_file),
        )
        .route("/api/review", get(get_review_queue))
        .route("/api/review/:id/accept", post(accept_file))
        .route("/api/review/:id/reject", post(reject_file))
        .route("/api/move", post(start_move))
        .route("/api/move/status", get(get_move_status))
        .route("/api/ripper/drives", get(list_drives))
        .route("/api/ripper/:index/start", post(start_rip))
        .route("/api/ripper/:index/cancel", post(cancel_rip))
        .route("/api/config", get(get_config).put(update_config))
        .route("/api/sse", get(sse_endpoint))
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

#[derive(Serialize)]
pub struct MoveResult {
    pub moved: Vec<String>,
    pub errors: Vec<MoveError>,
}

#[derive(Serialize)]
pub struct MoveError {
    pub id: String,
    pub filename: String,
    pub error: String,
}

async fn start_move(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartMoveBody>,
) -> Result<Json<ApiResponse<MoveResult>>, (StatusCode, String)> {
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

    let movies_path = std::path::PathBuf::from(&movies_dir);
    let tv_path = std::path::PathBuf::from(&tv_dir);

    let files = state
        .db
        .get_files_by_status("Confirmed")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state
        .sse
        .send_status(&format!("move:starting:{}", files.len()));

    let mover = app::MoverService::new();
    let mut moved = Vec::new();
    let mut errors = Vec::new();

    for file in &files {
        let id = file.id.clone();
        let filename = file.filename.clone();

        match mover.move_file(file, &movies_path, &tv_path).await {
            Ok(updated) => {
                if let Err(e) = state.db.update_file(&updated).await {
                    errors.push(MoveError {
                        id,
                        filename,
                        error: e.to_string(),
                    });
                } else {
                    moved.push(id);
                }
            }
            Err(e) => {
                errors.push(MoveError {
                    id,
                    filename,
                    error: e.to_string(),
                });
            }
        }
    }

    state.sse.send_status(&format!(
        "move:complete:moved={},errors={}",
        moved.len(),
        errors.len()
    ));

    Ok(Json(ApiResponse::ok(MoveResult { moved, errors })))
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

// --- Ripper ---

async fn list_drives(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<Vec<infra::DriveInfo>>> {
    let client = match &state.mkv_client {
        Some(c) => c,
        None => return Json(ApiResponse::ok(vec![])),
    };

    match client.detect_drives().await {
        Ok(drives) => Json(ApiResponse::ok(drives)),
        Err(e) => {
            info!("Failed to detect drives: {}", e);
            Json(ApiResponse::ok(vec![]))
        }
    }
}

async fn start_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    let client = state.mkv_client.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "MakeMKV is not installed".to_string(),
    ))?;

    let dest = {
        let config = state.config.read().await;
        config
            .scan_directory
            .clone()
            .unwrap_or_else(|| ".".to_string())
    };

    let mut handle = client
        .start_rip(index, &dest)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Spawn a task that runs the rip and broadcasts progress via SSE
    let sse = state.sse.clone();
    tokio::spawn(async move {
        sse.send_status(&format!("rip:starting:{}", index));

        while let Some(event) = handle.next_event().await {
            match event {
                infra::RipEvent::Progress {
                    current,
                    total,
                    max,
                } => {
                    let pct = if max > 0 {
                        (current as f64 / total as f64 * 100.0) as u32
                    } else {
                        0
                    };
                    sse.send_status(&format!("rip:progress:{}:{}", index, pct));
                }
                infra::RipEvent::Complete => {
                    sse.send_status(&format!("rip:complete:{}", index));
                    break;
                }
                infra::RipEvent::Error(e) => {
                    sse.send_status(&format!("rip:error:{}:{}", index, e));
                    break;
                }
                infra::RipEvent::Message(msg) => {
                    sse.send_status(&format!("rip:message:{}:{}", index, msg));
                }
            }
        }
    });

    Ok(Json(ApiResponse::ok(format!(
        "Rip started on drive {}",
        index
    ))))
}

async fn cancel_rip(
    State(_state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<String>> {
    // Cancel is handled by dropping the RipHandle.
    // For web-initiated rips, the spawned task holds the handle.
    // TODO: store active RipHandles for external cancellation.
    info!("Cancel requested for drive {} (not yet implemented)", index);
    Json(ApiResponse::ok(format!(
        "Drive {} cancel not yet implemented",
        index
    )))
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

    let updated = config.clone();
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
