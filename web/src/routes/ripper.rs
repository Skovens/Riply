use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use riply_core::{DriveInfo, DriveState, MetadataSearcher, RipEvent, RipHandle};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::api::{ApiResponse, AppState, DiscNameInfo};
use crate::error::WebError;

/// Auto-cleanup: remove contents of the temp rip directory that are older than the configured age.
/// This ensures stale temp rips from previous sessions don't accumulate, while preserving
/// recent rips for manual organization.
pub fn cleanup_temp_rips(temp_dir: &str, age_days: u64) {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    if let Ok(entries) = fs::read_dir(temp_dir) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age_threshold = age_days * 24 * 60 * 60;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        let age = now - modified.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                        if age > age_threshold {
                            if let Err(e) = fs::remove_dir_all(&path) {
                                warn!(path = %path.display(), error = %e, "Failed to cleanup temp rip folder (age: {} days)", age / 86400);
                            } else {
                                info!(path = %path.display(), "Cleaned up temp rip folder (age: {} days)", age / 86400);
                            }
                        } else {
                            info!(path = %path.display(), age_days = age / 86400, "Keeping temp rip folder (not old enough to cleanup)");
                        }
                    }
                }
            }
        }
    }
}

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

/// Estimate total rip size in bytes based on disc type heuristics.
/// DVD = 4.7 GB, dual-layer DVD = 8.5 GB, Blu-ray = 25/50 GB.
/// We use a conservative estimate since actual output is usually smaller.
fn estimate_total_bytes(num_titles: u32, has_dvd: bool) -> u64 {
    if num_titles == 0 {
        return 0;
    }
    // Per-title estimate: DVD title ~4GB, Blu-ray title ~20GB
    if has_dvd {
        num_titles as u64 * 4_000_000_000u64 // ~4GB per DVD title
    } else {
        num_titles as u64 * 20_000_000_000u64 // ~20GB per Blu-ray title
    }
}

/// Calculate total bytes written to a directory by summing all file sizes.
fn dir_total_bytes(dir: &str) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    // Recurse into subdirectories (MakeMKV creates DvdvolumeXX folders)
                    total += dir_total_bytes(&entry.path().to_string_lossy());
                }
            }
        }
    }
    total
}

/// Shared event loop that processes rip progress events and sends SSE updates.
/// Extracted to avoid duplication between `start_rip` and `execute_rip`.
///
/// When `output_dir` is provided, a file-size-growth fallback is used: if
/// PRGV progress lines are not emitted (e.g. Flatpak MakeMKV), the function
/// polls the output directory size every 2 seconds and computes percentage
/// from the growth rate.
async fn process_rip_events(
    handle: &mut RipHandle,
    sse: &crate::SseManager,
    index: usize,
    output_dir: Option<String>,
) -> Result<(), String> {
    let mut progress_start: Option<Instant> = None;
    let mut last_update: Option<Instant> = None;
    let mut current_pct: f64 = 0.0;
    let mut rate: Option<f64> = None;
    let mut prgv_active = true; // true until we confirm PRGV events are coming
    let mut titles_seen: u32 = 0;
    let mut has_dvd = true; // assume DVD until we see Blu-ray indicators

    // File-size fallback state
    let fs_start = Instant::now();
    let mut fs_last_bytes: Option<u64> = None;
    let mut fs_last_time: Option<Instant> = None;
    let mut fs_last_pct: u32 = 0;

    // Poll interval for file-size-based progress
    let poll_interval = std::time::Duration::from_secs(2);

    loop {
        // Wait for either an event from makemkvcon or a poll tick
        let event = tokio::select! {
            biased; // prefer actual events over poll ticks
            Some(event) = handle.next_event() => event,
            _ = tokio::time::sleep(poll_interval) => {
                // Poll tick — only emit progress if PRGV has been inactive
                if !prgv_active {
                    if let Some(ref dir) = output_dir {
                        let written = dir_total_bytes(dir);
                        let now = Instant::now();

                        // Estimate total once we have some data
                        let total_est = if titles_seen > 0 {
                            estimate_total_bytes(titles_seen, has_dvd)
                        } else {
                            0
                        };

                        if total_est > 0 && written > 0 {
                            let pct = (written as f64 / total_est as f64 * 100.0).clamp(0.0, 100.0) as u32;

                            // Calculate ETA from growth rate
                            let eta = if let (Some(prev_bytes), Some(prev_time)) = (fs_last_bytes, fs_last_time) {
                                let delta_bytes = written.saturating_sub(prev_bytes);
                                let delta_secs = now.duration_since(prev_time).as_secs_f64();
                                if delta_secs > 0.0 && delta_bytes > 0 {
                                    let bytes_per_sec = delta_bytes as f64 / delta_secs;
                                    let remaining = (total_est as f64 - written as f64).max(0.0);
                                    let secs_left = (remaining / bytes_per_sec).ceil() as u64;
                                    if secs_left > 0 && now.duration_since(fs_start).as_secs() >= 5 {
                                        Some(secs_left)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            // Only emit if percentage changed
                            if pct != fs_last_pct && pct > 0 {
                                fs_last_pct = pct;
                                let event_data = if let Some(eta) = eta {
                                    json!({"kind":"rip","action":"progress","index":index,"data":{"pct":pct,"eta":eta,"stage":"ripping"}})
                                } else {
                                    json!({"kind":"rip","action":"progress","index":index,"data":{"pct":pct,"eta":null,"stage":"ripping"}})
                                };
                                sse.send_json(event_data);
                            }

                            fs_last_bytes = Some(written);
                            fs_last_time = Some(now);
                        }
                    }
                }
                continue;
            }
        };

        match event {
            RipEvent::Progress {
                current,
                total,
                max,
            } => {
                prgv_active = true;
                fs_last_bytes = None; // reset file-size tracking when PRGV is active

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

                let event = if let Some(eta) = eta_secs {
                    json!({"kind":"rip","action":"progress","index":index,"data":{"pct":pct,"eta":eta,"stage":""}})
                } else {
                    json!({"kind":"rip","action":"progress","index":index,"data":{"pct":pct,"eta":null,"stage":""}})
                };
                sse.send_json(event);
            }
            RipEvent::Complete => {
                sse.send_json(json!({"kind":"rip","action":"complete","index":index,"data":{}}));
                return Ok(());
            }
            RipEvent::Error(e) => {
                sse.send_json(json!({"kind":"rip","action":"error","index":index,"data":{"message":e.to_string()}}));
                return Err(e);
            }
            RipEvent::Message(msg) => {
                sse.send_json(json!({"kind":"rip","action":"message","index":index,"data":{"text":msg}}));

                // Check for "Saving N titles" message to kick off file-size fallback
                if msg.contains("Saving") && msg.contains("titles") {
                    // Extract number of titles
                    if let Some(num_str) = msg.split_whitespace().nth(1) {
                        if let Ok(n) = num_str.parse::<u32>() {
                            titles_seen = n;
                        }
                    }
                    // Check for Blu-ray indicators
                    has_dvd = !msg.to_lowercase().contains("bluray") && !msg.to_lowercase().contains("bd");
                }
            }
        }
    }
}

/// If `candidate` (a full directory path) already exists and contains files,
/// return a unique path by appending ` (Copy 1)`, ` (Copy 2)` etc.
/// Prevents MakeMKV from blocking on its interactive overwrite prompt.
/// `subdir_name` is the human-readable stem (e.g. "Inception (2010)").
fn _unique_rip_destination(candidate: &str, subdir_name: Option<&str>) -> String {
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
/// For movies, searches TMDb/OMDb and uses the matched title.
/// For TV episodes, also verifies the ripped video duration against TMDb.
async fn resolve_rip_title_with_duration(
    disc_name: &str,
    rip_folder: &str,
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

    // Search for the TV show
    let metadata = if let Ok(results) = searcher
        .search(&search_title, search_year, &riply_core::MediaType::Tv)
        .await
    {
        results.into_iter().next()
    } else {
        None
    };

    // Check if this is a TV episode and verify duration
    if let Some(m) = &metadata {
        if m.is_tv {
            let season = m.season_number.unwrap_or(0);
            let episode = m.episode_number.unwrap_or(0);
            let tmdb_id = m.tmdb_id.unwrap_or(0);

            if season > 0 && episode > 0 && tmdb_id > 0 {
                // Find the largest video file in the rip folder
                let video_files = app::find_video_files(std::path::Path::new(rip_folder));
                if let Some(largest) = video_files.last() {
                    // Fetch expected duration from TMDb
                    if let Ok(Some(expected_duration)) = searcher
                        .fetch_episode_duration(tmdb_id, season, episode)
                        .await
                    {
                        // Verify duration — non-blocking, non-fatal
                        let _ = app::verify_episode_duration(largest, expected_duration);
                    }
                }
            }
        }
    }

    if let Some(m) = metadata {
        (m.title.clone(), m.year.map(|y| y as u16))
    } else {
        (title, year)
    }
}

/// Apply "(Copy N)" suffix if the destination already exists.
fn apply_copy_n(path: &std::path::Path) -> std::path::PathBuf {
    if path.exists() {
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        let stem = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Disc".to_string());
        let mut copy_n = 1;
        loop {
            let copy_name = format!("{} (Copy {})", stem, copy_n);
            let copy_path = parent.join(&copy_name);
            if !copy_path.exists() {
                return copy_path;
            }
            copy_n += 1;
        }
    }
    path.to_path_buf()
}

pub async fn start_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    Json(_): Json<StartRipBody>,
) -> Result<Json<ApiResponse<String>>, WebError> {
    let backend = state.backend.as_ref().ok_or_else(|| {
        WebError::ServiceUnavailable(
            "No ripping backend available. Install makemkvcon or Flatpak package.".to_string(),
        )
    })?;

    // Resolve disc name for the temp folder
    let backend_clone = Arc::clone(backend);
    let drives = tokio::task::spawn_blocking(move || backend_clone.detect_drives())
        .await
        .unwrap_or(Ok(vec![]))
        .unwrap_or(vec![]);

    let disc_name = drives.iter().find(|d| d.index == index).and_then(|d| d.disc_name.clone());

    // Create temp rip folder: {temp_directory}/{drive_index}/
    let config = state.config.read().await;
    let temp_dir = config.temp_directory.clone();
    drop(config);
    let temp_folder = format!("{}/{}", temp_dir, index);
    std::fs::create_dir_all(&temp_folder)
        .map_err(|e| WebError::Internal(format!("Failed to create temp folder: {}", e)))?;

    let mut handle = backend
        .start_rip(index, &temp_folder)
        .map_err(|e| WebError::Internal(e.to_string()))?;

    let sse = state.sse.clone();
    let searcher = state.searcher.clone();
    let state_clone = Arc::clone(&state);
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(());
    let rip_handle = tokio::spawn(async move {
        sse.send_json(json!({"kind":"rip","action":"starting","index":index,"data":{}}));

        // Process rip progress events, checking for cancellation
        let rip_result = tokio::select! {
            result = process_rip_events(&mut handle, &sse, index, Some(temp_folder.clone())) => result,
            _ = cancel_rx.changed() => {
                // Cancellation requested - cancel the makemkvcon process
                let _ = handle.cancel().await;
                Err("Rip cancelled".to_string())
            }
        };

        // Remove from active rips (whether success or error)
        state_clone.active_rips.write().await.remove(&index);

        // Post-rip: concurrent autodetection during rip is done above;
        // now handle the result
        if let Ok(()) = rip_result {
            // Successful rip — attempt auto-naming and duration verification
            if let Some(ref disc) = disc_name {
                let auto_name = resolve_rip_title_with_duration(disc, &temp_folder, searcher.as_deref()).await;
                if !auto_name.0.is_empty() {
                    let year = auto_name.1.map(|y| y as u32);
                    sse.send_json(json!({"kind":"rip","action":"named","index":index,"data":{"title":auto_name.0,"year":year,"type":"movie"}}));
                } else {
                    sse.send_json(json!({"kind":"rip","action":"naming_failed","index":index,"data":{}}));
                }
            } else {
                sse.send_json(json!({"kind":"rip","action":"naming_failed","index":index,"data":{}}));
            }

            // Move to Organizer scan directory for processing
            // The Organizer window will handle the final rename/move
            let organizer_dir = {
                let config = state_clone.config.read().await;
                config
                    .rip_output_directory
                    .clone()
                    .unwrap_or_else(|| ".".to_string())
            };

            let folder_name = disc_name.clone().unwrap_or_else(|| format!("Disc_{}", index));
            let final_dest = std::path::Path::new(&organizer_dir).join(&folder_name);
            let final_dest_str = apply_copy_n(&final_dest).to_string_lossy().to_string();

            match std::fs::rename(&temp_folder, &final_dest_str) {
                Ok(()) => {
                    info!(
                        from = %temp_folder,
                        to = %final_dest_str,
                        "Post-rip move to Organizer completed"
                    );
                    sse.send_json(json!({"kind":"rip","action":"complete","index":index,"data":{"dest":final_dest_str}}));
                }
                Err(e) => {
                    warn!(error = %e, from = %temp_folder, to = %final_dest_str, "Post-rip move failed");
                    sse.send_json(json!({"kind":"rip","action":"message","index":index,"data":{"text":format!("Move failed: {}", e)}}));
                }
            }
        } else {
            // Rip failed — clean up temp folder
            info!(rip_folder = %temp_folder, "Cleaning up failed rip");
            let _ = std::fs::remove_dir_all(&temp_folder);
            sse.send_json(json!({"kind":"rip","action":"message","index":index,"data":{"text":"Rip failed, cleaned up"}}));
        }
    });

    state.active_rips.write().await.insert(index, (cancel_tx, rip_handle));

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
    if let Some((cancel_tx, task_handle)) = rips.remove(&index) {
        // Send cancellation signal to the task (which will cancel makemkvcon)
        let _ = cancel_tx.send(());
        // Then abort the tokio task
        task_handle.abort();
        state.sse.send_json(json!({"kind":"rip","action":"cancelled","index":index,"data":{}}));
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

/// Polls drives for state changes and broadcasts SSE events.
/// Also handles auto-rip when a disc is inserted on an autorip-enabled drive.
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

        let backend = match &state.backend {
            Some(b) => Arc::clone(b),
            None => continue,
        };

        // Detect drives (blocking)
        let drives = match tokio::task::spawn_blocking(move || backend.detect_drives()).await {
            Ok(Ok(drives)) => drives,
            _ => continue,
        };

        // Broadcast drive state changes via SSE (all drives, not just autorip)
        let enabled = state.autorip_drives.read().await;
        let sse = &state.sse;
        for drive in &drives {
            let prev = prev_states.insert(drive.index, drive.state.clone());
            match (prev, &drive.state) {
                (Some(DriveState::Empty) | None, &DriveState::Ready) => {
                    sse.send_json(json!({"kind":"drive","action":"inserted","index":drive.index,"data":{}}));
                }
                (Some(DriveState::Ready), &DriveState::Empty) => {
                    sse.send_json(json!({"kind":"drive","action":"ejected","index":drive.index,"data":{}}));
                }
                _ => {}
            }
        }

        // Track removed drives so we emit a removal event
        let current_indices: std::collections::HashSet<usize> =
            drives.iter().map(|d| d.index).collect();
        for idx in prev_states.keys() {
            if !current_indices.contains(idx) {
                sse.send_json(json!({"kind":"drive","action":"removed","index":idx,"data":{}}));
            }
        }
        drop(enabled);

        // Auto-rip: only watch autorip-enabled drives
        if state.autorip_drives.read().await.is_empty() {
            continue;
        }

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

            // Resolve disc name for the temp folder
            let temp_dir = {
                let config = state.config.read().await;
                config.temp_directory.clone()
            };
            let temp_folder = format!("{}/{}", temp_dir, drive.index);
            if std::fs::create_dir_all(&temp_folder).is_err() {
                continue;
            }

            // Start the rip in a background task
            let state_clone = Arc::clone(&state);
            let drive_index = drive.index;
            let eject_after = autorip_eject;
            let disc_name = drive.disc_name.clone();

            let backend = match &state.backend {
                Some(b) => Arc::clone(b),
                None => continue,
            };

            let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(());

            let rip_task = {
                let handle = match backend.start_rip(drive_index, &temp_folder) {
                    Ok(h) => h,
                    Err(e) => {
                        warn!(drive = drive_index, error = %e, "Auto-rip failed to start");
                        continue;
                    }
                };

                let state_clone_2 = Arc::clone(&state_clone);
                tokio::spawn(async move {
                    // Process rip with cancellation support
                    let rip_result = tokio::select! {
                        result = execute_rip(state_clone_2.clone(), drive_index, handle, disc_name.clone(), temp_folder.clone(), eject_after) => result,
                        _ = cancel_rx.changed() => {
                            // Cancellation requested - cancel the makemkvcon process
                            Err("Rip cancelled".to_string())
                        }
                    };

                    if let Err(e) = rip_result {
                        tracing::error!(drive = drive_index, error = %e, "Auto-rip failed");
                    }
                })
            };

            drop(active);
            drop(enabled);
            state.active_rips.write().await.insert(drive.index, (cancel_tx, rip_task));
            break;
        }
    }
}

/// Execute a rip on a drive, handling progress events and optional eject.
/// Used by both manual and auto-rip paths. The caller manages active_rips.
async fn execute_rip(
    state: Arc<AppState>,
    index: usize,
    mut handle: RipHandle,
    disc_name: Option<String>,
    temp_folder: String,
    eject_after: bool,
) -> Result<(), String> {
    let sse = state.sse.clone();
    sse.send_json(json!({"kind":"rip","action":"starting","index":index,"data":{}}));

    let searcher = state.searcher.clone();
    let state_clone = Arc::clone(&state);
    let result = process_rip_events(&mut handle, &sse, index, Some(temp_folder.clone())).await;

    // Post-rip: move or cleanup
    match result {
        Ok(()) => {
            // Successful rip — attempt auto-naming and duration verification
            if let Some(ref disc) = disc_name {
                let auto_name = resolve_rip_title_with_duration(disc, &temp_folder, searcher.as_deref()).await;
                if !auto_name.0.is_empty() {
                    let year = auto_name.1.map(|y| y as u32);
                    sse.send_json(json!({"kind":"rip","action":"named","index":index,"data":{"title":auto_name.0,"year":year,"type":"movie"}}));
                } else {
                    sse.send_json(json!({"kind":"rip","action":"naming_failed","index":index,"data":{}}));
                }
            } else {
                sse.send_json(json!({"kind":"rip","action":"naming_failed","index":index,"data":{}}));
            }

            // Move to Organizer scan directory for processing
            let organizer_dir = {
                let config = state_clone.config.read().await;
                config
                    .rip_output_directory
                    .clone()
                    .unwrap_or_else(|| ".".to_string())
            };

            let folder_name = disc_name.clone().unwrap_or_else(|| format!("Disc_{}", index));
            let final_dest = std::path::Path::new(&organizer_dir).join(&folder_name);
            let final_dest_str = apply_copy_n(&final_dest).to_string_lossy().to_string();

            match std::fs::rename(&temp_folder, &final_dest_str) {
                Ok(()) => {
                    info!(
                        from = %temp_folder,
                        to = %final_dest_str,
                        "Post-rip move to Organizer completed"
                    );
                    sse.send_json(json!({"kind":"rip","action":"complete","index":index,"data":{"dest":final_dest_str}}));
                }
                Err(e) => {
                    warn!(error = %e, from = %temp_folder, to = %final_dest_str, "Post-rip move failed");
                    sse.send_json(json!({"kind":"rip","action":"message","index":index,"data":{"text":format!("Move failed: {}", e)}}));
                }
            }
        }
        Err(_) => {
            // Rip failed — clean up temp folder
            info!(rip_folder = %temp_folder, "Cleaning up failed rip");
            let _ = std::fs::remove_dir_all(&temp_folder);
            sse.send_json(json!({"kind":"rip","action":"message","index":index,"data":{"text":"Rip failed, cleaned up"}}));
        }
    }

    // Eject if configured
    if eject_after {
        let backend_eject = state
            .backend
            .as_ref()
            .ok_or("No ripping backend")?;
        let backend_eject = Arc::clone(backend_eject);
        let drives = tokio::task::spawn_blocking(move || backend_eject.detect_drives())
            .await
            .unwrap_or(Ok(vec![]))
            .unwrap_or(vec![]);

        if let Some(drive) = drives.iter().find(|d| d.index == index) {
            if let Some(ref device_path) = drive.device_path {
                let path = device_path.clone();
                let _ = tokio::task::spawn_blocking(move || infra::eject_disc(&path)).await;
                sse.send_json(json!({"kind":"autorip","action":"ejected","index":index,"data":{}}));
                info!(drive = index, "Auto-rip ejected disc");
            }
        }
    }

    Ok(())
}
