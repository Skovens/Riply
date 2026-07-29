use std::sync::Arc;

use crate::api::{ApiResponse, AppState};
use crate::error::WebError;
use axum::{
    extract::{Path, State},
    Json,
};
use riply_core::FileRecord;

pub async fn get_review_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<FileRecord>>>, WebError> {
    let files = state
        .db
        .get_files_by_status("Needs Review")
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(files)))
}

pub async fn accept_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FileRecord>>, WebError> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
        .ok_or_else(|| WebError::NotFound(format!("File not found: {}", id)))?;

    let valid = matches!(
        file.status,
        riply_core::PipelineStatus::Matched | riply_core::PipelineStatus::NeedsReview
    );
    if !valid {
        return Err(WebError::BadRequest(format!(
            "Cannot accept file in '{}' status (must be Matched or NeedsReview)",
            file.status
        )));
    }

    file.update_status(riply_core::PipelineStatus::Confirmed);

    state
        .db
        .update_file(&file)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    state.sse.send_status(&format!("review:accepted:{}", id));

    Ok(Json(ApiResponse::ok(file)))
}

pub async fn reject_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FileRecord>>, WebError> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
        .ok_or_else(|| WebError::NotFound(format!("File not found: {}", id)))?;

    if file.status != riply_core::PipelineStatus::NeedsReview {
        return Err(WebError::BadRequest(format!(
            "Cannot reject file in '{}' status (must be NeedsReview)",
            file.status
        )));
    }

    file.update_status(riply_core::PipelineStatus::Failed);

    state
        .db
        .update_file(&file)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    state.sse.send_status(&format!("review:rejected:{}", id));

    Ok(Json(ApiResponse::ok(file)))
}
