use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use riply_core::{DriveInfo, DriveState, MetadataSearcher, RipEvent, RipHandle};
use serde::Deserialize;
use tracing::{info, warn};

use crate::api::{ApiResponse, AppState};
use crate::error::WebError;

/// Router for the ripper domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/ripper/drives", get(list_drives))
        .route("/api/ripper/status", get(ripper_status))
        .route("/api/ripper/:index/start", post(start_rip))
        .route("/api/ripper/:index/cancel", post(cancel_rip))
        .route("/api/ripper/:index/eject", post(eject_disc))
        .route("/api/ripper/autorip", get(autorip_status))
        .route(
            "/api/ripper/autorip/:index/toggle",
            post(autorip_toggle),
        )
        .with_state(state)
}

pub async fn ripper_status() -> Json<ApiResponse<infra::InstallationStatus>> {
    let status = tokio::task::spawn_blocking(infra::check_installation)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "check_installation panicked");
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
) -> Result<Json<ApiResponse<Vec<DriveInfo>>>, WebError> {
    let backend = match &state.backend {
        Some(b) => Arc::clone(b),
        None => return Ok(Json(ApiResponse::ok(vec![]))),
    };

    let drives = tokio::task::spawn_blocking(move || backend.detect_drives())
        .await
        .map_err(|e| WebError::Internal(format!("Task failed: {}", e)))?;

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
            warn!(error = %e, "Failed to detect drives");
            Err(WebError::Internal(format!(
                "Failed to detect drives: {}",
                e
            )))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StartRipBody {
    pub destination: Option<String>,
}

fn calculate_eta(
    pct: u32,
    progress_start: &mut Option<Instant>,
    last_update: &mut Option<Instant>,
    current_pct: &mut f64,
    rate: &mut Option<f64>,
) -> Option<u64> {
    if pct == 0 || pct >= 100 {
        return None;
    }

    let now = Instant::now();
    let start = *progress_start.get_or_insert(now);

    if last_update.is_none() {
        *last_update = Some(now);
        *current_pct = pct as f64;
        return None;
    }

    let elapsed = now.duration_since(start).as_secs_f64();
    let new_pct = pct as f64;

    if new_pct != *current_pct && new_pct > *current_pct {
        let delta_time = now.duration_since(last_update.unwrap()).as_secs_f64();
        if delta_time > 0.0 {
            let pct_done = (new_pct - *current_pct) / 100.0;
            *rate = Some(pct_done / delta_time);
            *current_pct = new_pct;
            *last_update = Some(now);
        }
    }

    if let Some(r) = *rate {
        if r > 0.0 {
            let remaining = ((100.0 - new_pct) / 100.0) / r;
            if remaining > 0.0 && elapsed >= 3.0 {
                return Some(remaining.ceil() as u64);
            }
        }
    }

    None
}

/// Shared event loop that processes rip progress events and sends SSE updates.
/// Extracted to avoid duplication between `start_rip` and `execute_rip`.
async fn process_rip_events(
    handle: &mut RipHandle,
    sse: &crate::SseManager,
    index: usize,
) -> Result<(), String> {
    let mut progress_start: Option<Instant> = None;
    let mut last_update: Option<Instant> = None;
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

                let eta_secs = calculate_eta(
                    pct,
                    &mut progress_start,
                    &mut last_update,
                    &mut current_pct,
                    &mut rate,
                );

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
                return Ok(());
            }
            RipEvent::Error(e) => {
                sse.send_status(&format!("rip:error:{}:{}", index, e));
                return Err(e);
            }
            RipEvent::Message(msg) => {
                sse.send_status(&format!("rip:message:{}:{}", index, msg));
            }
        }
    }
    Ok(())
}

/// Resolve a disc name to a title (+ year) for the output directory.
/// If a metadata searcher is available, search TMDb/OMDb and use the
/// matched title; otherwise fall back to the raw parsed disc name.
async fn resolve_rip_title(
    disc_name: &str,
    searcher: Option<&dyn MetadataSearcher>,
) -> (String, Option<u16>) {
    let (title, year) = infra::makemkv::parser::parse_disc_name(disc_name);
    if title.is_empty() {
        return (title, year);
    }

    let Some(searcher) = searcher else {
        return (title, year);
    };

    let search_title = riply_core::NameCleaner::clean_name_for_search(&title);
    let search_year = year.map(u32::from);

    if let Ok(results) = searcher
        .search(&search_title, search_year, &riply_core::MediaType::Movie)
        .await
    {
        if let Some(m) = results.first() {
            return (m.title.clone(), m.year.map(|y| y as u16));
        }
    }

    (title, year)
}

pub async fn start_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    Json(body): Json<StartRipBody>,
) -> Result<Json<ApiResponse<String>>, WebError> {
    let backend = state.backend.as_ref().ok_or_else(|| {
        WebError::ServiceUnavailable(
            "No ripping backend available. Install makemkvcon or Flatpak package.".to_string(),
        )
    })?;

    let explicit_dest = body.destination.as_ref().filter(|d| !d.is_empty()).cloned();

    let mut dest = match &explicit_dest {
        Some(d) => d.clone(),
        None => {
            let config = state.config.read().await;
            config
                .validate_for_ripping()
                .map_err(WebError::BadRequest)?;
            config
                .rip_output_directory
                .clone()
                .unwrap_or_else(|| ".".to_string())
        }
    };

    // If no explicit destination, look up disc name, search metadata, and create subdirectory
    if explicit_dest.is_none() {
        let backend_clone = Arc::clone(backend);
        let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
            .await
            .unwrap_or(Ok(vec![]))
            .unwrap_or(vec![]);

        if let Some(drive) = drives.iter().find(|d| d.index == index) {
            if let Some(ref disc_name) = drive.disc_name {
                let (title, year) = resolve_rip_title(disc_name, state.searcher.as_deref()).await;
                if !title.is_empty() {
                    dest = match year {
                        Some(y) => format!("{}/{} ({})", dest, title, y),
                        None => format!("{}/{}", dest, title),
                    };
                }
            }
        }
    }

    let mut handle = backend
        .start_rip(index, &dest)
        .map_err(|e| WebError::Internal(e.to_string()))?;

    let sse = state.sse.clone();
    let state_clone = Arc::clone(&state);
    let rip_handle = tokio::spawn(async move {
        sse.send_status(&format!("rip:starting:{}", index));

        if let Err(e) = process_rip_events(&mut handle, &sse, index).await {
            tracing::error!(drive = index, error = %e, "Rip failed");
        }

        state_clone.active_rips.write().await.remove(&index);
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
        info!(drive = index, "Cancelled rip");
        Json(ApiResponse::ok(format!("Drive {} rip cancelled", index)))
    } else {
        info!(drive = index, "No active rip found");
        Json(ApiResponse::ok(format!("No active rip on drive {}", index)))
    }
}

pub async fn eject_disc(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Result<Json<ApiResponse<String>>, WebError> {
    let backend = match &state.backend {
        Some(b) => Arc::clone(b),
        None => {
            return Err(WebError::ServiceUnavailable(
                "No ripping backend available".to_string(),
            ))
        }
    };

    let drives = tokio::task::spawn_blocking(move || backend.detect_drives())
        .await
        .map_err(|e| WebError::Internal(format!("Task failed: {}", e)))?
        .map_err(|e| WebError::Internal(e.to_string()))?;

    let device_path = drives
        .iter()
        .find(|d| d.index == index)
        .and_then(|d| d.device_path.clone())
        .ok_or_else(|| {
            WebError::NotFound(format!("Drive {} not found or has no device path", index))
        })?;

    let device_path_log = device_path.clone();
    tokio::task::spawn_blocking(move || infra::eject_disc(&device_path))
        .await
        .map_err(|e| WebError::Internal(format!("Task failed: {}", e)))?
        .map_err(|e| WebError::Internal(e.to_string()))?;

    info!(drive = index, device = %device_path_log, "Ejected disc");
    Ok(Json(ApiResponse::ok(format!(
        "Ejected disc from drive {}",
        index
    ))))
}

pub async fn autorip_status(State(state): State<Arc<AppState>>) -> Json<ApiResponse<Vec<usize>>> {
    let drives = state.autorip_drives.read().await;
    let mut enabled: Vec<usize> = drives.iter().copied().collect();
    enabled.sort_unstable();
    Json(ApiResponse::ok(enabled))
}

pub async fn autorip_toggle(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Result<Json<ApiResponse<Vec<usize>>>, WebError> {
    let mut drives = state.autorip_drives.write().await;
    if drives.contains(&index) {
        drives.remove(&index);
        info!(drive = index, "Auto-rip disabled for drive");
    } else {
        drives.insert(index);
        info!(drive = index, "Auto-rip enabled for drive");
    }
    let mut enabled: Vec<usize> = drives.iter().copied().collect();
    enabled.sort_unstable();
    Ok(Json(ApiResponse::ok(enabled)))
}

/// Background polling loop that detects disc insertion on drives with auto-rip
/// enabled and starts ripping them. Runs for the lifetime of the server but
/// does no work (no `detect_drives` calls) while a rip is active or when no
/// drive has auto-rip enabled.
pub async fn autorip_poll_loop(state: Arc<AppState>) {
    use riply_core::DriveState;
    use std::collections::HashMap;
    use std::time::Duration;

    let mut prev_states: HashMap<usize, DriveState> = HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Pause detection while a rip is active: the current rip is never
        // interrupted, and other drives are picked up once it finishes.
        if !state.active_rips.read().await.is_empty() {
            continue;
        }

        // Nothing to watch if no drive has auto-rip enabled.
        if state.autorip_drives.read().await.is_empty() {
            continue;
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

        let autorip_eject = state.config.read().await.ripping.autorip_eject_after;
        let active = state.active_rips.read().await;
        let enabled = state.autorip_drives.read().await;

        // Drop tracking for drives that no longer have auto-rip enabled, so
        // re-enabling treats the current disc as a fresh insertion.
        prev_states.retain(|idx, _| enabled.contains(idx));

        for drive in &drives {
            if !enabled.contains(&drive.index) {
                continue;
            }

            let prev = prev_states.insert(drive.index, drive.state.clone());

            // Detect Empty → Ready transition (disc inserted), or first sighting
            // of a Ready drive (auto-rip just enabled with a disc present).
            let is_new_disc = matches!(prev, Some(DriveState::Empty) | None)
                && matches!(drive.state, DriveState::Ready);

            if !is_new_disc {
                continue;
            }

            // Skip if already ripping on this drive
            if active.contains_key(&drive.index) {
                continue;
            }

            info!(
                drive = drive.index,
                disc = %drive.disc_name.as_deref().unwrap_or("unknown"),
                "Auto-rip: disc detected"
            );

            // Start the rip in a background task
            let state_clone = Arc::clone(&state);
            let drive_index = drive.index;
            let eject_after = autorip_eject;

            let handle = tokio::spawn(async move {
                if let Err(e) = execute_rip(state_clone.clone(), drive_index, eject_after).await {
                    tracing::error!(drive = drive_index, error = %e, "Auto-rip failed");
                }
            });

            drop(active);
            drop(enabled);
            state.active_rips.write().await.insert(drive.index, handle);
            break;
        }
    }
}

/// Execute a rip on a drive, handling progress events and optional eject.
/// Used by both manual and auto-rip paths.
async fn execute_rip(state: Arc<AppState>, index: usize, eject_after: bool) -> Result<(), String> {
    let backend = state.backend.as_ref().ok_or("No ripping backend")?;

    // Resolve destination
    let dest = {
        let config = state.config.read().await;
        config.validate_for_ripping()?;
        let mut d = config
            .rip_output_directory
            .clone()
            .unwrap_or_else(|| ".".to_string());

        // Auto-subdirectory from disc name with metadata search
        let backend_clone = Arc::clone(backend);
        let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
            .await
            .unwrap_or(Ok(vec![]))
            .unwrap_or(vec![]);

        if let Some(drive) = drives.iter().find(|d| d.index == index) {
            if let Some(ref disc_name) = drive.disc_name {
                let (title, year) = resolve_rip_title(disc_name, state.searcher.as_deref()).await;
                if !title.is_empty() {
                    d = match year {
                        Some(y) => format!("{}/{} ({})", d, title, y),
                        None => format!("{}/{}", d, title),
                    };
                }
            }
        }
        d
    };

    let mut handle = backend.start_rip(index, &dest).map_err(|e| e.to_string())?;

    let sse = state.sse.clone();
    sse.send_status(&format!("rip:starting:{}", index));

    let result = process_rip_events(&mut handle, &sse, index).await;

    // Remove from active rips (whether success or error)
    state.active_rips.write().await.remove(&index);

    result?;

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
                info!(drive = index, "Auto-rip ejected disc");
            }
        }
    }

    Ok(())
}
