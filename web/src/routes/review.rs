use std::sync::Arc;

use crate::api::{ApiResponse, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use riply_core::FileRecord;

pub async fn get_review_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<FileRecord>>>, (StatusCode, String)> {
    let files = state
        .db
        .get_files_by_status("Needs Review")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(files)))
}

pub async fn accept_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FileRecord>>, (StatusCode, String)> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("File not found: {}", id)))?;

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

pub async fn reject_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FileRecord>>, (StatusCode, String)> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("File not found: {}", id)))?;

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
