use std::sync::Arc;

use axum::{extract::State, Json};
use infra::Config;
use serde::Deserialize;

use crate::api::{ApiResponse, AppState};
use crate::error::WebError;

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
    pub healthchecks_uuid: Option<String>,
    pub healthchecks_ping_interval_minutes: Option<u64>,
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConfigUpdateBody>,
) -> Result<Json<ApiResponse<Config>>, WebError> {
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
    if let Some(ref v) = body.healthchecks_uuid {
        config.healthchecks.uuid = v.clone();
    }
    if let Some(v) = body.healthchecks_ping_interval_minutes {
        config.healthchecks.ping_interval_minutes = v;
    }

    let updated = config.clone();

    if let Some(ref path) = state.config_path.clone() {
        if let Err(e) = updated.save(path) {
            tracing::warn!("Failed to save config to {}: {}", path, e);
        }
    }

    // Dynamically restart healthchecks heartbeat if its config changed
    if body.healthchecks_uuid.is_some() || body.healthchecks_ping_interval_minutes.is_some() {
        if let Some(handle) = state.healthchecks_handle.write().await.take() {
            handle.abort();
        }
        if !config.healthchecks.uuid.is_empty() {
            if let Some(hc) =
                infra::healthchecks::HealthchecksClient::from_config(&config.healthchecks)
            {
                hc.start().await;
                let interval =
                    std::time::Duration::from_secs(config.healthchecks.ping_interval_minutes * 60);
                let handle = tokio::spawn(async move {
                    let mut timer = tokio::time::interval(interval);
                    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        timer.tick().await;
                        hc.ping().await;
                    }
                });
                *state.healthchecks_handle.write().await = Some(handle);
            }
        }
    }

    Ok(Json(ApiResponse::ok(updated)))
}
