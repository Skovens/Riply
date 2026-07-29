use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use crate::api::{ApiResponse, AppState};
use crate::error::WebError;

#[derive(Serialize)]
pub struct JobInfoResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub progress_total: usize,
    pub progress_processed: usize,
}

pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<Vec<JobInfoResponse>>> {
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

pub async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<JobInfoResponse>>, WebError> {
    let job = state
        .scheduler
        .get_job_status(&id)
        .await
        .map_err(|e| WebError::NotFound(e.to_string()))?;

    Ok(Json(ApiResponse::ok(JobInfoResponse {
        id: job.id,
        kind: job.kind.to_string(),
        status: job.status.to_string(),
        progress_total: job.progress.total,
        progress_processed: job.progress.processed,
    })))
}

pub async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, WebError> {
    state
        .scheduler
        .cancel_job(&id)
        .await
        .map_err(|e| WebError::NotFound(e.to_string()))?;

    state.sse.send_status(&format!("job:cancelled:{}", id));
    Ok(Json(ApiResponse::ok(id)))
}
