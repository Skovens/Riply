use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use riply_core::FileRecord;
use serde::Deserialize;

use crate::api::{ApiResponse, AppState, PaginatedResponse};

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
) -> Result<Json<ApiResponse<PaginatedResponse<FileRecord>>>, (StatusCode, String)> {
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
) -> Result<Json<ApiResponse<FileRecord>>, (StatusCode, String)> {
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

pub async fn update_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateFileBody>,
) -> Result<Json<ApiResponse<FileRecord>>, (StatusCode, String)> {
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

pub async fn delete_file(
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
