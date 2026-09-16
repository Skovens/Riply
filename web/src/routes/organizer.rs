// ADR-026: Folder-Based Pipeline — Organizer API routes

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use riply_core::{FolderRecord, FolderStore};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::api::{ApiResponse, AppState};
use crate::error::WebError;

/// Router for the organizer domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/folders", get(list_folders))
        .route("/v1/folders/stats", get(folder_stats))
        .route("/v1/folders/scan", post(scan_folders))
        .route("/v1/folders/:id", get(get_folder).delete(delete_folder))
        .route("/v1/folders/:id/analyze", post(analyze_folder))
        .route("/v1/folders/:id/accept", post(accept_folder))
        .route("/v1/folders/:id/reject", post(reject_folder))
        .route("/v1/folders/accept-all", post(accept_all_folders))
        .route("/v1/folders/reject-all", post(reject_all_folders))
        .route("/v1/folders/:id/move", post(move_folder))
        .route("/v1/folders/:id/preview", get(preview_move))
        .with_state(state)
}

// ── Request/Response types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ScanFoldersBody {
    /// Directory to scan for folders
    pub directory: String,
}

#[derive(Debug, Serialize)]
pub struct ScanFoldersResponse {
    pub folder_count: usize,
    pub folders: Vec<FolderRecord>,
}

#[derive(Debug, Serialize)]
pub struct FolderStatsResponse {
    pub by_status: Vec<(String, i64)>,
}

#[derive(Debug, Serialize)]
pub struct MovePreviewResponse {
    pub destination: String,
}

// ── Handlers ────────────────────────────────────────────────────────────────────

/// List all folders, optionally filtered by status
async fn list_folders(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<FolderRecord>>>, WebError> {
    let folders = state
        .db
        .get_all_folders()
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(folders)))
}

/// Get folder statistics by status
async fn folder_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<FolderStatsResponse>>, WebError> {
    let by_status = state
        .db
        .count_folders_by_status()
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(FolderStatsResponse { by_status })))
}

/// Scan a directory for folders containing media files
async fn scan_folders(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ScanFoldersBody>,
) -> Result<Json<ApiResponse<ScanFoldersResponse>>, WebError> {
    let dir = std::path::PathBuf::from(&body.directory);
    if !dir.exists() {
        return Err(WebError::BadRequest(format!(
            "Directory does not exist: {}",
            body.directory
        )));
    }

    // Build organizer service with metadata searcher if available
    let searcher = state.searcher.clone();
    let folder_store: Arc<dyn riply_core::FolderStore> = state.db.clone();
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(
        searcher.map(|s| s as Arc<dyn riply_core::MetadataSearcher>),
        folder_store,
        movies_dir,
        tv_dir,
    );

    let result = organizer
        .scan_directory(&dir)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(count = result.folders.len(), dir = %body.directory, "Folder scan complete");

    Ok(Json(ApiResponse::ok(ScanFoldersResponse {
        folder_count: result.folders.len(),
        folders: result.folders,
    })))
}

/// Get a single folder by ID
async fn get_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FolderRecord>>, WebError> {
    let folder = state
        .db
        .get_folder_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
        .ok_or_else(|| WebError::NotFound(format!("Folder not found: {}", id)))?;

    Ok(Json(ApiResponse::ok(folder)))
}

/// Analyze a folder — run TMDb search and update record
async fn analyze_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FolderRecord>>, WebError> {
    let folder = state
        .db
        .get_folder_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
        .ok_or_else(|| WebError::NotFound(format!("Folder not found: {}", id)))?;

    let searcher = state.searcher.clone();
    let folder_store: Arc<dyn riply_core::FolderStore> = state.db.clone();
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(
        searcher.map(|s| s as Arc<dyn riply_core::MetadataSearcher>),
        folder_store,
        movies_dir,
        tv_dir,
    );

    let result = organizer
        .analyze_folder(&folder)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(result)))
}

/// Accept a folder — move to Confident status
async fn accept_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FolderRecord>>, WebError> {
    let folder_store: Arc<dyn riply_core::FolderStore> = state.db.clone();
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(None, folder_store, movies_dir, tv_dir);

    let result = organizer
        .accept_folder(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(folder_id = %id, "Folder accepted");

    Ok(Json(ApiResponse::ok(result)))
}

/// Reject a folder — move to Unknown status
async fn reject_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FolderRecord>>, WebError> {
    let folder_store: Arc<dyn riply_core::FolderStore> = state.db.clone();
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(None, folder_store, movies_dir, tv_dir);

    let result = organizer
        .reject_folder(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(folder_id = %id, "Folder rejected");

    Ok(Json(ApiResponse::ok(result)))
}

/// Accept all confident folders
async fn accept_all_folders(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<usize>>, WebError> {
    let folder_store: Arc<dyn riply_core::FolderStore> = state.db.clone();
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(None, folder_store, movies_dir, tv_dir);

    let count = organizer
        .accept_all_folders()
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(count, "Accepted all confident folders");

    Ok(Json(ApiResponse::ok(count)))
}

/// Reject all unknown folders — move to Quarantined
async fn reject_all_folders(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<usize>>, WebError> {
    let folder_store: Arc<dyn riply_core::FolderStore> = state.db.clone();
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(None, folder_store, movies_dir, tv_dir);

    let count = organizer
        .reject_all_folders()
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(count, "Rejected all unknown folders");

    Ok(Json(ApiResponse::ok(count)))
}

/// Move a single confident folder to its destination
async fn move_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, WebError> {
    let folder_store: Arc<dyn riply_core::FolderStore> = state.db.clone();
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(None, folder_store, movies_dir, tv_dir);

    organizer
        .move_folder(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(folder_id = %id, "Folder moved");

    Ok(Json(ApiResponse::ok(())))
}

/// Preview where a folder would move to
async fn preview_move(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<MovePreviewResponse>>, WebError> {
    let folder = state
        .db
        .get_folder_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
        .ok_or_else(|| WebError::NotFound(format!("Folder not found: {}", id)))?;

    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory_buf(), config.tv_directory_buf())
    };

    let organizer = app::OrganizerService::new(None, state.db.clone(), movies_dir, tv_dir);

    let destination = organizer
        .preview_move(&folder)
        .ok_or_else(|| WebError::Internal("Could not compute preview".to_string()))?;

    Ok(Json(ApiResponse::ok(MovePreviewResponse { destination })))
}

/// Delete a folder by ID
async fn delete_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, WebError> {
    state
        .db
        .delete_folder(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(folder_id = %id, "Folder deleted");

    Ok(Json(ApiResponse::ok(())))
}
