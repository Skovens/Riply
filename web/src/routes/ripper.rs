use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use riply_core::{DriveInfo, RipEvent};
use serde::Deserialize;
use tracing::info;

use crate::api::{ApiResponse, AppState};

pub async fn ripper_status() -> Json<ApiResponse<infra::InstallationStatus>> {
    let status = infra::check_installation();
    Json(ApiResponse::ok(status))
}

pub async fn list_drives(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<DriveInfo>>>, (StatusCode, String)> {
    let backend = match &state.backend {
        Some(b) => b,
        None => return Ok(Json(ApiResponse::ok(vec![]))),
    };

    match backend.detect_drives() {
        Ok(drives) => Ok(Json(ApiResponse::ok(drives))),
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

    let dest = match body.destination {
        Some(d) => d,
        None => {
            let config = state.config.read().await;
            config
                .scan_directory
                .clone()
                .unwrap_or_else(|| ".".to_string())
        }
    };

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
