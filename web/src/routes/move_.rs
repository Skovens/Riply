use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use crate::api::{ApiResponse, AppState, JobStartedResponse};
use crate::error::WebError;

/// Router for the move domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/move", post(start_move))
        .route("/v1/move/status", get(get_move_status))
        .route("/v1/move/preview", get(preview_move))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct StartMoveBody {
    pub movies_dir: Option<String>,
    pub tv_dir: Option<String>,
}

#[derive(Serialize)]
pub struct MoveStatusResponse {
    pub confirmed_count: i64,
    pub moved_count: i64,
}

#[derive(Serialize)]
pub struct MovePreviewItem {
    pub id: String,
    pub filename: String,
    pub source_path: String,
    pub dest_path: String,
    pub media_type: String,
}

pub async fn start_move(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartMoveBody>,
) -> Result<Json<ApiResponse<JobStartedResponse>>, WebError> {
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
                    sse.send_json(json!({"kind":"move","action":"complete","index":0,"data":{"job_id":"move","moved":0,"errors":0}}));
                    return Ok(());
                }

                progress.set_total(files.len());
                sse.send_json(json!({"kind":"move","action":"starting","index":0,"data":{"total":files.len(),"job_id":"move"}}));

                let mover = app::MoverService::new();
                let mut moved = 0;
                let mut errors = 0;

                for (i, file) in files.iter().enumerate() {
                    if cancel.is_cancelled() {
                        sse.send_json(json!({"kind":"move","action":"cancelled","index":0,"data":{"job_id":"move"}}));
                        return Ok(());
                    }
                    match mover.move_file(file, &movies_path, &tv_path).await {
                        Ok(updated) => {
                            if let Err(e) = db.update_file(&updated).await {
                                sse.send_log(
                                    "warn",
                                    &format!("Failed to update {}: {}", file.filename(), e),
                                );
                                errors += 1;
                            } else {
                                moved += 1;
                            }
                        }
                        Err(e) => {
                            sse.send_log(
                                "warn",
                                &format!("Failed to move {}: {}", file.filename(), e),
                            );
                            errors += 1;
                        }
                    }
                    progress.increment();
                    if (i + 1) % 10 == 0 || i + 1 == files.len() {
                        sse.send_json(json!({"kind":"move","action":"progress","index":0,"data":{"job_id":"move","processed":i+1,"total":files.len()}}));
                    }
                }

                sse.send_json(json!({"kind":"move","action":"complete","index":0,"data":{"job_id":"move","moved":moved,"errors":errors}}));
                info!(moved, errors, "Move complete");
                Ok(())
            }
        })
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(JobStartedResponse {
        job_id,
        status: "Queued".to_string(),
    })))
}

pub async fn get_move_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<MoveStatusResponse>>, WebError> {
    let confirmed = state
        .db
        .count_files_by_status("Confirmed")
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    let moved = state
        .db
        .count_files_by_status("Moved")
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(MoveStatusResponse {
        confirmed_count: confirmed,
        moved_count: moved,
    })))
}

pub async fn preview_move(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<MovePreviewItem>>>, WebError> {
    let (movies_dir, tv_dir) = {
        let config = state.config.read().await;
        (config.movies_directory.clone(), config.tv_directory.clone())
    };

    let files = state
        .db
        .get_files_by_status("Confirmed")
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    let mover = app::MoverService::new();
    let mut previews = Vec::new();

    for file in &files {
        match mover
            .preview_move(file, &PathBuf::from(&movies_dir), &PathBuf::from(&tv_dir))
            .await
        {
            Ok(dest_path) => previews.push(MovePreviewItem {
                id: file.id().to_string(),
                filename: file.filename().to_string(),
                source_path: file.path().to_string_lossy().to_string(),
                dest_path: dest_path.to_string_lossy().to_string(),
                media_type: file.media_type().to_string(),
            }),
            Err(e) => {
                tracing::warn!(file = %file.filename(), error = %e, "Failed to preview move");
            }
        }
    }

    Ok(Json(ApiResponse::ok(previews)))
}
