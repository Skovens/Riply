use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use infra::Config;
use serde::Deserialize;

use crate::api::{ApiResponse, AppState};

pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<ApiResponse<Config>> {
    let config = state.config.read().await;
    Json(ApiResponse::ok(config.clone()))
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdateBody {
    pub database_path: Option<String>,
    pub scan_directory: Option<String>,
    pub movies_directory: Option<String>,
    pub tv_directory: Option<String>,
    pub rip_output_directory: Option<String>,
    pub verbose: Option<bool>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub tmdb_api_key: Option<String>,
    pub omdb_api_key: Option<String>,
    pub ripping_command: Option<String>,
    pub autorip_enabled: Option<bool>,
    pub autorip_eject_after: Option<bool>,
    pub autorip_excluded_drives: Option<Vec<usize>>,
}

pub async fn update_config(
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
    if let Some(v) = body.rip_output_directory {
        config.rip_output_directory = Some(v);
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
    if let Some(v) = body.tmdb_api_key {
        config.tmdb_api_key = Some(v);
    }
    if let Some(v) = body.omdb_api_key {
        config.omdb_api_key = Some(v);
    }
    if let Some(v) = body.ripping_command {
        config.ripping.command = Some(v);
    }
    if let Some(v) = body.autorip_enabled {
        config.ripping.autorip_enabled = v;
    }
    if let Some(v) = body.autorip_eject_after {
        config.ripping.autorip_eject_after = v;
    }
    if let Some(v) = body.autorip_excluded_drives {
        config.ripping.autorip_excluded_drives = Some(v);
    }

    let updated = config.clone();

    if let Some(ref path) = state.config_path.clone() {
        if let Err(e) = updated.save(path) {
            tracing::warn!("Failed to save config to {}: {}", path, e);
        }
    }

    Ok(Json(ApiResponse::ok(updated)))
}
