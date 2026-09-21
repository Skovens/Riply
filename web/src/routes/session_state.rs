use std::sync::Arc;

use axum::{
    extract::State,
    routing::{delete, get},
    Json, Router,
};

use crate::api::{ApiResponse, AppState};
use crate::error::WebError;

/// Router for the session state domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/session-state", get(list_session_states))
        .route("/v1/session-state", delete(clear_session_states))
        .with_state(state)
}

#[derive(Debug, serde::Serialize)]
pub struct SessionStateResponse {
    pub drive_index: usize,
    pub last_title: String,
    pub last_year: Option<i32>,
    pub last_season: Option<i32>,
    pub last_episode: Option<i32>,
    pub last_media_type: String,
    pub last_tmdb_id: Option<i32>,
}

pub async fn list_session_states(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<SessionStateResponse>>>, WebError> {
    let states = state
        .db
        .list_session_states()
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;
    let responses = states
        .into_iter()
        .map(|s| SessionStateResponse {
            drive_index: s.drive_index,
            last_title: s.last_title,
            last_year: s.last_year,
            last_season: s.last_season,
            last_episode: s.last_episode,
            last_media_type: s.last_media_type,
            last_tmdb_id: s.last_tmdb_id,
        })
        .collect();
    Ok(Json(ApiResponse::ok(responses)))
}

pub async fn clear_session_states(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<()>>, WebError> {
    state
        .db
        .clear_session_states()
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;
    Ok(Json(ApiResponse::ok(())))
}
