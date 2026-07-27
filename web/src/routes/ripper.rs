use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use riply_core::{DriveInfo, DriveState, RipEvent};
use serde::Deserialize;
use tracing::info;

use crate::api::{ApiResponse, AppState};

pub async fn ripper_status() -> Json<ApiResponse<infra::InstallationStatus>> {
    let status = tokio::task::spawn_blocking(infra::check_installation)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("check_installation panicked: {}", e);
            infra::InstallationStatus {
                installed: false,
                method: infra::DetectionMethod::None,
                flatpak_available: false,
                flathub_configured: false,
                flatpak_permissions_ok: false,
                drive_access_ok: false,
                missing: vec![infra::MissingComponent::MakeMKV],
            }
        });
    Json(ApiResponse::ok(status))
}

pub async fn list_drives(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<DriveInfo>>>, (StatusCode, String)> {
    let backend = match &state.backend {
        Some(b) => Arc::clone(b),
        None => return Ok(Json(ApiResponse::ok(vec![]))),
    };

    let drives = tokio::task::spawn_blocking(move || backend.detect_drives())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Task failed: {}", e),
            )
        })?;

    match drives {
        Ok(mut drives) => {
            let rips = state.active_rips.read().await;
            for drive in &mut drives {
                if rips.contains_key(&drive.index) {
                    drive.state = DriveState::Ripping;
                }
            }
            Ok(Json(ApiResponse::ok(drives)))
        }
        Err(e) => {
            info!("Failed to detect drives: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to detect drives: {}", e),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StartRipBody {
    pub destination: Option<String>,
}

pub async fn start_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    Json(body): Json<StartRipBody>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    let backend = state.backend.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "No ripping backend available. Install makemkvcon or Flatpak package.".to_string(),
    ))?;

    let explicit_dest = body.destination.as_ref().filter(|d| !d.is_empty()).cloned();

    let mut dest = match &explicit_dest {
        Some(d) => d.clone(),
        None => {
            let config = state.config.read().await;
            config
                .validate_for_ripping()
                .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
            config
                .rip_output_directory
                .clone()
                .unwrap_or_else(|| ".".to_string())
        }
    };

    // If no explicit destination, look up disc name and create subdirectory
    if explicit_dest.is_none() {
        let backend_clone = Arc::clone(backend);
        let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
            .await
            .unwrap_or(Ok(vec![]))
            .unwrap_or(vec![]);

        if let Some(drive) = drives.iter().find(|d| d.index == index) {
            if let Some(ref disc_name) = drive.disc_name {
                let (title, _year) = infra::makemkv::parser::parse_disc_name(disc_name);
                if !title.is_empty() {
                    dest = format!("{}/{}", dest, title);
                }
            }
        }
    }

    let mut handle = backend
        .start_rip(index, &dest)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sse = state.sse.clone();
    let rip_handle = tokio::spawn(async move {
        sse.send_status(&format!("rip:starting:{}", index));

        while let Some(event) = handle.next_event().await {
            match event {
                RipEvent::Progress {
                    current,
                    total,
                    max,
                } => {
                    let pct = if total > 0 && max > 0 {
                        (current as f64 / total as f64 * 100.0) as u32
                    } else {
                        0
                    };
                    sse.send_status(&format!("rip:progress:{}:{}", index, pct));
                }
                RipEvent::Complete => {
                    sse.send_status(&format!("rip:complete:{}", index));
                    break;
                }
                RipEvent::Error(e) => {
                    sse.send_status(&format!("rip:error:{}:{}", index, e));
                    break;
                }
                RipEvent::Message(msg) => {
                    sse.send_status(&format!("rip:message:{}:{}", index, msg));
                }
            }
        }
    });

    state.active_rips.write().await.insert(index, rip_handle);

    Ok(Json(ApiResponse::ok(format!(
        "Rip started on drive {}",
        index
    ))))
}

pub async fn cancel_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Json<ApiResponse<String>> {
    let mut rips = state.active_rips.write().await;
    if let Some(handle) = rips.remove(&index) {
        handle.abort();
        state.sse.send_status(&format!("rip:cancelled:{}", index));
        info!("Cancelled rip on drive {}", index);
        Json(ApiResponse::ok(format!("Drive {} rip cancelled", index)))
    } else {
        info!("No active rip found for drive {}", index);
        Json(ApiResponse::ok(format!("No active rip on drive {}", index)))
    }
}

pub async fn eject_disc(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    let backend = match &state.backend {
        Some(b) => Arc::clone(b),
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "No ripping backend available".to_string(),
            ))
        }
    };

    let drives = tokio::task::spawn_blocking(move || backend.detect_drives())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Task failed: {}", e),
            )
        })?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let device_path = drives
        .iter()
        .find(|d| d.index == index)
        .and_then(|d| d.device_path.clone())
        .ok_or((
            StatusCode::NOT_FOUND,
            format!("Drive {} not found or has no device path", index),
        ))?;

    let device_path_log = device_path.clone();
    tokio::task::spawn_blocking(move || infra::eject_disc(&device_path))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Task failed: {}", e),
            )
        })?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!("Ejected disc from drive {} ({})", index, device_path_log);
    Ok(Json(ApiResponse::ok(format!(
        "Ejected disc from drive {}",
        index
    ))))
}
