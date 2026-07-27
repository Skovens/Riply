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
        use std::time::Instant;

        sse.send_status(&format!("rip:starting:{}", index));

        let mut progress_start: Option<Instant> = None;
        let mut current_pct: f64 = 0.0;
        let mut rate: Option<f64> = None;

        while let Some(event) = handle.next_event().await {
            match event {
                RipEvent::Progress {
                    current,
                    total,
                    max,
                } => {
                    let pct = if total > 0 && max > 0 {
                        (current as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as u32
                    } else {
                        0
                    };

                    // Calculate ETA with persisted rate across events
                    let eta_secs = if pct > 0 && pct < 100 {
                        let now = Instant::now();
                        if let Some(start) = progress_start {
                            let elapsed = now.duration_since(start).as_secs_f64();
                            let new_pct = pct as f64;

                            if new_pct != current_pct && new_pct > current_pct && elapsed > 0.0 {
                                let pct_done = (new_pct - current_pct) / 100.0;
                                if pct_done > 0.0 {
                                    rate = Some(pct_done / elapsed);
                                }
                                current_pct = new_pct;
                            }

                            if let Some(r) = rate {
                                let remaining = (1.0 - new_pct / 100.0) / r;
                                if remaining > 0.0 && elapsed >= 3.0 {
                                    Some(remaining as u64)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            progress_start = Some(now);
                            current_pct = pct as f64;
                            None
                        }
                    } else {
                        None
                    };

                    match eta_secs {
                        Some(eta) => {
                            sse.send_status(&format!("rip:progress:{}:{}:{}", index, pct, eta));
                        }
                        None => {
                            sse.send_status(&format!("rip:progress:{}:{}", index, pct));
                        }
                    }
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

pub async fn autorip_status(State(state): State<Arc<AppState>>) -> Json<ApiResponse<bool>> {
    let handle = state.autorip_handle.read().await;
    Json(ApiResponse::ok(handle.is_some()))
}

pub async fn autorip_toggle(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, String)> {
    let mut handle = state.autorip_handle.write().await;
    if handle.is_some() {
        handle.take();
        info!("Auto-rip disabled");
        Ok(Json(ApiResponse::ok(false)))
    } else {
        let state_clone = Arc::clone(&state);
        let new_handle = tokio::spawn(async move {
            autorip_poll_loop(state_clone).await;
        });
        *handle = Some(new_handle);
        info!("Auto-rip enabled");
        Ok(Json(ApiResponse::ok(true)))
    }
}

/// Background polling loop that detects disc insertion and auto-starts ripping.
pub async fn autorip_poll_loop(state: Arc<AppState>) {
    use riply_core::DriveState;
    use std::collections::HashMap;
    use std::time::Duration;

    let mut prev_states: HashMap<usize, DriveState> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        // Check if we should stop
        if state.autorip_handle.read().await.is_none() {
            info!("Auto-rip polling stopped");
            break;
        }

        let backend = match &state.backend {
            Some(b) => Arc::clone(b),
            None => continue,
        };

        // Detect drives (blocking)
        let drives = match tokio::task::spawn_blocking(move || backend.detect_drives()).await {
            Ok(Ok(drives)) => drives,
            _ => continue,
        };

        // Check for config on each iteration (in case it changed)
        let (autorip_enabled, autorip_eject, excluded) = {
            let config = state.config.read().await;
            (
                config.ripping.autorip_enabled,
                config.ripping.autorip_eject_after,
                config.ripping.autorip_excluded_drives.clone(),
            )
        };

        if !autorip_enabled {
            break;
        }

        let active = state.active_rips.read().await;

        for drive in &drives {
            let prev = prev_states.get(&drive.index).cloned();
            prev_states.insert(drive.index, drive.state.clone());

            // Push state changes to frontend via SSE
            if prev != Some(drive.state.clone()) {
                state.sse.send_status(&format!("drive:changed:{}:{}", drive.index, drive.state));
            }

            // Detect Empty → Ready transition (disc inserted)
            let is_new_disc = matches!(prev, Some(DriveState::Empty) | None)
                && matches!(drive.state, DriveState::Ready);

            if !is_new_disc {
                continue;
            }

            // Skip if already ripping on this drive
            if active.contains_key(&drive.index) {
                continue;
            }

            // Skip if drive is in the excluded list
            if let Some(ref excluded_list) = excluded {
                if excluded_list.contains(&drive.index) {
                    continue;
                }
            }

            info!(
                "Auto-rip: disc detected on drive {} ({})",
                drive.index,
                drive.disc_name.as_deref().unwrap_or("unknown")
            );

            // Start the rip in a background task
            let state_clone = Arc::clone(&state);
            let drive_index = drive.index;
            let eject_after = autorip_eject;

            let handle = tokio::spawn(async move {
                if let Err(e) = execute_rip(state_clone.clone(), drive_index, eject_after).await {
                    tracing::error!("Auto-rip failed on drive {}: {}", drive_index, e);
                }
            });

            drop(active);
            state.active_rips.write().await.insert(drive.index, handle);
            break;
        }
    }
}

/// Execute a rip on a drive, handling progress events and optional eject.
/// Used by both manual and auto-rip paths.
async fn execute_rip(state: Arc<AppState>, index: usize, eject_after: bool) -> Result<(), String> {
    use std::time::Instant;

    let backend = state.backend.as_ref().ok_or("No ripping backend")?;

    // Resolve destination
    let dest = {
        let config = state.config.read().await;
        config.validate_for_ripping()?;
        let mut d = config
            .rip_output_directory
            .clone()
            .unwrap_or_else(|| ".".to_string());

        // Auto-subdirectory from disc name
        let backend_clone = Arc::clone(backend);
        let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
            .await
            .unwrap_or(Ok(vec![]))
            .unwrap_or(vec![]);

        if let Some(drive) = drives.iter().find(|d| d.index == index) {
            if let Some(ref disc_name) = drive.disc_name {
                let (title, _year) = infra::makemkv::parser::parse_disc_name(disc_name);
                if !title.is_empty() {
                    d = format!("{}/{}", d, title);
                }
            }
        }
        d
    };

    let mut handle = backend.start_rip(index, &dest).map_err(|e| e.to_string())?;

    let sse = state.sse.clone();
    sse.send_status(&format!("rip:starting:{}", index));

    let mut progress_start: Option<Instant> = None;
    let mut current_pct: f64 = 0.0;
    let mut rate: Option<f64> = None;

    while let Some(event) = handle.next_event().await {
        match event {
            riply_core::RipEvent::Progress {
                current,
                total,
                max,
            } => {
                let pct = if total > 0 && max > 0 {
                    (current as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as u32
                } else {
                    0
                };

                let eta_secs = if pct > 0 && pct < 100 {
                    let now = Instant::now();
                    if let Some(start) = progress_start {
                        let elapsed = now.duration_since(start).as_secs_f64();
                        let new_pct = pct as f64;

                        if new_pct != current_pct && new_pct > current_pct && elapsed > 0.0 {
                            let pct_done = (new_pct - current_pct) / 100.0;
                            if pct_done > 0.0 {
                                rate = Some(pct_done / elapsed);
                            }
                            current_pct = new_pct;
                        }

                        if let Some(r) = rate {
                            let remaining = (1.0 - new_pct / 100.0) / r;
                            if remaining > 0.0 && elapsed >= 3.0 {
                                Some(remaining as u64)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        progress_start = Some(now);
                        current_pct = pct as f64;
                        None
                    }
                } else {
                    None
                };

                match eta_secs {
                    Some(eta) => {
                        sse.send_status(&format!("rip:progress:{}:{}:{}", index, pct, eta))
                    }
                    None => sse.send_status(&format!("rip:progress:{}:{}", index, pct)),
                }
            }
            riply_core::RipEvent::Complete => {
                sse.send_status(&format!("rip:complete:{}", index));
                break;
            }
            riply_core::RipEvent::Error(e) => {
                sse.send_status(&format!("rip:error:{}:{}", index, e));
                return Err(e);
            }
            riply_core::RipEvent::Message(msg) => {
                sse.send_status(&format!("rip:message:{}:{}", index, msg));
            }
        }
    }

    // Remove from active rips
    state.active_rips.write().await.remove(&index);

    // Eject if configured
    if eject_after {
        let backend_clone = Arc::clone(backend);
        let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
            .await
            .unwrap_or(Ok(vec![]))
            .unwrap_or(vec![]);

        if let Some(drive) = drives.iter().find(|d| d.index == index) {
            if let Some(ref device_path) = drive.device_path {
                let path = device_path.clone();
                let _ = tokio::task::spawn_blocking(move || infra::eject_disc(&path)).await;
                sse.send_status(&format!("autorip:ejected:{}", index));
                info!("Auto-rip: ejected disc from drive {}", index);
            }
        }
    }

    Ok(())
}
