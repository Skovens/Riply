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

use crate::api::{ApiResponse, AppState, DiscNameInfo};
use crate::error::WebError;

/// Request body for the rename endpoint
#[derive(Debug, Deserialize)]
pub struct RenameDriveBody {
    pub title: String,
    pub year: Option<u16>,
    pub media_type: Option<String>,
}

/// Router for the ripper domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/ripper/drives", get(list_drives))
        .route("/v1/ripper/drives/:index/rename", post(rename_disc))
        .route("/v1/ripper/status", get(ripper_status))
        .route("/v1/ripper/:index/start", post(start_rip))
        .route("/v1/ripper/:index/cancel", post(cancel_rip))
        .route("/v1/ripper/:index/eject", post(eject_disc))
        .route("/v1/ripper/autorip", get(autorip_status))
        .route("/v1/ripper/autorip/:index/toggle", post(autorip_toggle))
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
            let disc_names = state.disc_names.read().await;
            for drive in &mut drives {
                if rips.contains_key(&drive.index) {
                    drive.state = DriveState::Ripping;
                }
                // Apply user-set name if it exists
                if let Some(info) = disc_names.get(&drive.index) {
                    drive.derived_name = Some(info.final_name.clone());
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

/// Rename a disc's final name. Called when the user selects a media match
/// from the search results via the naming modal.
pub async fn rename_disc(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    Json(body): Json<RenameDriveBody>,
) -> Result<Json<ApiResponse<()>>, WebError> {
    let backend = state
        .backend
        .as_ref()
        .ok_or(WebError::NotFound("No ripping backend".to_string()))?;

    // Verify the drive exists and has a disc
    let backend_clone = Arc::clone(backend);
    let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
        .await
        .map_err(|e| WebError::Internal(format!("Task failed: {}", e)))?
        .map_err(|e| WebError::Internal(e.to_string()))?;

    let drive = drives
        .iter()
        .find(|d| d.index == index)
        .ok_or_else(|| WebError::NotFound(format!("Drive {} not found", index)))?;

    let disc_name = drive
        .disc_name
        .clone()
        .ok_or_else(|| WebError::NotFound(format!("No disc in drive {}", index)))?;

    let final_name = match body.year {
        Some(y) => format!("{} ({})", body.title, y),
        None => body.title.clone(),
    };

    info!(
        drive = index,
        disc = %disc_name,
        final_name = %final_name,
        "User set disc name"
    );

    state.disc_names.write().await.insert(
        index,
        DiscNameInfo {
            disc_name,
            final_name,
        },
    );

    Ok(Json(ApiResponse::ok(())))
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

/// If `candidate` (a full directory path) already exists and contains files,
/// return a unique path by appending ` (Copy 1)`, ` (Copy 2)` etc.
/// Prevents MakeMKV from blocking on its interactive overwrite prompt.
/// `subdir_name` is the human-readable stem (e.g. "Inception (2010)").
fn unique_rip_destination(candidate: &str, subdir_name: Option<&str>) -> String {
    let candidate_path = std::path::Path::new(candidate);
    if candidate_path.exists() && candidate_path.is_dir() {
        let has_content = std::fs::read_dir(candidate_path)
            .map(|mut iter| iter.next().is_some())
            .unwrap_or(false);
        if has_content {
            let parent = candidate_path.parent().unwrap_or(candidate_path);
            let stem = subdir_name.unwrap_or("Disc");
            let mut copy_n = 1;
            loop {
                let copy_name = format!("{} (Copy {})", stem, copy_n);
                let copy_path = parent.join(&copy_name);
                if !copy_path.exists() {
                    return copy_path.to_string_lossy().to_string();
                }
                copy_n += 1;
            }
        }
    }
    candidate.to_string()
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

    let (base, subdir_hint) = match &explicit_dest {
        Some(d) => {
            // User explicitly named the destination; use as-is but uniquify
            let p = std::path::Path::new(d);
            let parent = p.parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| ".".to_string());
            let name = p.file_name().map(|x| x.to_string_lossy().to_string());
            (parent, name)
        }
        None => {
            let config = state.config.read().await;
            config
                .validate_for_ripping()
                .map_err(WebError::BadRequest)?;
            let r = config
                .rip_output_directory
                .clone()
                .unwrap_or_else(|| ".".to_string());

            // Look up disc name, search metadata, and create subdirectory
            let backend_clone = Arc::clone(backend);
            let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
                .await
                .unwrap_or(Ok(vec![]))
                .unwrap_or(vec![]);

            // Resolve title asynchronously
            let subdir: Option<String> = if let Some(drive) = drives.iter().find(|d| d.index == index) {
                if let Some(ref disc_name) = drive.disc_name {
                    let (title, year) = resolve_rip_title(disc_name, state.searcher.as_deref()).await;
                    if !title.is_empty() {
                        Some(match year {
                            Some(y) => format!("{} ({})", title, y),
                            None => title,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            match &subdir {
                Some(s) => (r.clone(), Some(s.clone())),
                None => (r, None),
            }
        }
    };

    let candidate = match &subdir_hint {
        Some(s) => format!("{}/{}", base, s),
        None => base.clone(),
    };

    let dest = unique_rip_destination(&candidate, subdir_hint.as_deref());

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

    // Resolve destination, auto-append (Copy N) if the directory already exists
    let (dest, final_name) = {
        let config = state.config.read().await;
        config.validate_for_ripping()?;
        let base = config
            .rip_output_directory
            .clone()
            .unwrap_or_else(|| ".".to_string());

        // Check for user-set name first
        let user_name = state.disc_names.read().await.get(&index).map(|info| info.final_name.clone());

        let subdir_name = if let Some(name) = user_name {
            // User set a name — use it directly
            Some(name)
        } else {
            // Fall back to derived name from drive detection
            let backend_clone = Arc::clone(backend);
            let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
                .await
                .unwrap_or(Ok(vec![]))
                .unwrap_or(vec![]);

            if let Some(drive) = drives.iter().find(|d| d.index == index) {
                // Use derived name if available, otherwise raw disc name
                if let Some(ref derived) = drive.derived_name {
                    Some(derived.clone())
                } else if let Some(ref disc_name) = drive.disc_name {
                    // No derived name — use raw disc name as fallback
                    let (title, year) = resolve_rip_title(disc_name, state.searcher.as_deref()).await;
                    if !title.is_empty() {
                        Some(match year {
                            Some(y) => format!("{} ({})", title, y),
                            None => title,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        let rip_folder_name = subdir_name.clone().unwrap_or_else(|| "Disc".to_string());

        let candidate = match &subdir_name {
            Some(ref s) => format!("{}/{}", base, s),
            None => base.clone(),
        };

        let dest = unique_rip_destination(&candidate, Some(&rip_folder_name));
        (dest, subdir_name)
    };

    let mut handle = backend.start_rip(index, &dest).map_err(|e| e.to_string())?;

    let sse = state.sse.clone();
    sse.send_status(&format!("rip:starting:{}", index));

    let result = process_rip_events(&mut handle, &sse, index).await;

    // Remove from active rips (whether success or error)
    state.active_rips.write().await.remove(&index);

    // Post-rip: move or cleanup
    let rip_folder = dest.clone();
    match result {
        Ok(()) => {
            // Successful rip — move if final name differs from rip folder
            if let Some(ref final_name) = final_name {
                // If the rip folder differs from the expected final folder, move it
                let rip_parent = std::path::Path::new(&rip_folder)
                    .parent()
                    .unwrap_or(std::path::Path::new("."));
                let dest_path = rip_parent.join(final_name);

                if dest_path != std::path::Path::new(&rip_folder) {
                    // Apply (Copy N) if destination exists
                    let final_dest = if dest_path.exists() {
                        let parent = dest_path.parent().unwrap_or(std::path::Path::new("."));
                        let mut copy_n = 1;
                        loop {
                            let copy_name = format!("{} (Copy {})", final_name, copy_n);
                            let copy_path = parent.join(&copy_name);
                            if !copy_path.exists() {
                                break copy_path.to_string_lossy().to_string();
                            }
                            copy_n += 1;
                        }
                    } else {
                        dest_path.to_string_lossy().to_string()
                    };

                    match std::fs::rename(&rip_folder, &final_dest) {
                        Ok(()) => {
                            info!(
                                from = %rip_folder,
                                to = %final_dest,
                                "Post-rip move completed"
                            );
                            sse.send_status(&format!("rip:message:{}:Moved to {}", index, final_name));
                        }
                        Err(e) => {
                            warn!(error = %e, from = %rip_folder, to = %final_dest, "Post-rip move failed");
                            sse.send_status(&format!("rip:message:{}:Move failed: {}", index, e));
                        }
                    }
                } else {
                    sse.send_status(&format!("rip:message:{}:Rip complete", index));
                }
            } else {
                // No final name — rip folder is the final state
                sse.send_status(&format!("rip:message:{}:Rip complete", index));
            }
        }
        Err(_) => {
            // Rip failed — clean up
            info!(rip_folder = %rip_folder, "Cleaning up failed rip");
            let _ = std::fs::remove_dir_all(&rip_folder);
            sse.send_status(&format!("rip:message:{}:Rip failed, cleaned up", index));
        }
    }

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
