use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use riply_core::FileRecord;
use serde::Deserialize;

use crate::api::{ApiResponse, AppState, PaginatedResponse};
use crate::error::WebError;

/// Router for the files domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/files", get(list_files).delete(reset_db))
        .route(
            "/v1/files/:id",
            get(get_file).post(update_file).delete(delete_file),
        )
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct FileListParams {
    pub status: Option<String>,
    pub search: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFileBody {
    pub canonical_title: Option<String>,
    pub canonical_year: Option<u32>,
    pub series_name: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub media_type: Option<String>,
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileListParams>,
) -> Result<Json<ApiResponse<PaginatedResponse<FileRecord>>>, WebError> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(200);

    let (items, total) = state
        .db
        .get_files_paginated(
            params.status.as_deref(),
            params.search.as_deref(),
            page,
            per_page,
        )
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(PaginatedResponse {
        items,
        total,
        page,
        per_page,
    })))
}

pub async fn get_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<FileRecord>>, WebError> {
    match state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
    {
        Some(file) => Ok(Json(ApiResponse::ok(file))),
        None => Err(WebError::NotFound(format!("File not found: {}", id))),
    }
}

pub async fn update_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateFileBody>,
) -> Result<Json<ApiResponse<FileRecord>>, WebError> {
    let mut file = state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
        .ok_or_else(|| WebError::NotFound(format!("File not found: {}", id)))?;

    if let Some(title) = body.canonical_title {
        *file.canonical_title_mut() = Some(title);
    }
    if let Some(year) = body.canonical_year {
        *file.canonical_year_mut() = Some(year);
    }
    if let Some(name) = body.series_name {
        *file.series_name_mut() = Some(name);
    }
    if let Some(season) = body.season_number {
        *file.season_number_mut() = Some(season);
    }
    if let Some(episode) = body.episode_number {
        *file.episode_number_mut() = Some(episode);
    }
    if let Some(mt) = body.media_type {
        *file.media_type_mut() = match mt.as_str() {
            "tv" | "Tv" | "TV" => riply_core::MediaType::Tv,
            _ => riply_core::MediaType::Movie,
        };
    }

    state
        .db
        .update_file(&file)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(file)))
}

pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, WebError> {
    if state
        .db
        .get_file_by_id(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(WebError::NotFound(format!("File not found: {}", id)));
    }

    state
        .db
        .delete_file(&id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(id)))
}

pub async fn reset_db(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<usize>>, WebError> {
    let deleted = state
        .db
        .clear_all()
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    state
        .sse
        .send_status(&format!("db:reset:deleted={}", deleted));

    Ok(Json(ApiResponse::ok(deleted)))
}
