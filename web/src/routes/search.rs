use std::sync::Arc;

use axum::{extract::Query, extract::State, routing::get, Json, Router};
use serde::Deserialize;
use tracing::info;

use crate::api::{ApiResponse, AppState};
use crate::error::WebError;
use riply_core::{MediaType, MetadataMatch};

/// Router for the search domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/search", get(search_metadata))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub r#type: Option<String>,
}

pub async fn search_metadata(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<ApiResponse<Vec<MetadataMatch>>>, WebError> {
    let Some(searcher) = state.searcher.as_ref() else {
        return Err(WebError::service_unavailable(
            "No metadata searcher configured",
        ));
    };

    let media_type = match query.r#type.as_deref() {
        Some("tv") => MediaType::Tv,
        _ => MediaType::Movie,
    };

    info!(query = %query.q, type = %media_type, "Search metadata");

    let results = searcher
        .search(&query.q, None, &media_type)
        .await
        .map_err(|e| WebError::internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(results)))
}
