//! Event handler for drive events.
//!
//! Receives events from the event source channel, processes them,
//! updates drive state, and sends SSE notifications.

use std::sync::Arc;

use infra::drive_events::DriveEvent;
use serde_json::json;
use tracing::info;

use super::ripper;
use crate::api::AppState;

/// Handle a drive insertion event.
pub async fn handle_drive_inserted(state: &Arc<AppState>, event: &DriveEvent) {
    let drive_index = event.drive_index();
    let device_path = event.device_path();
    let disc_name = match event {
        DriveEvent::Inserted { disc_name, .. } => disc_name.as_deref(),
        _ => None,
    };

    info!(
        drive_index,
        device_path = %device_path,
        disc_name = ?disc_name,
        "Drive insertion event received"
    );

    // Invalidate drives cache
    {
        let mut cache = state.drives_cache.write().await;
        *cache = None;
    }

    // Send SSE notification
    let sse_data = serde_json::json!({
        "type": "drive_inserted",
        "drive_index": drive_index,
        "device_path": device_path,
        "disc_name": disc_name,
    });

    state.sse.send_json(sse_data);

    // Auto-naming: try to identify disc via TMDb when a disc is inserted
    // This happens regardless of auto-rip status — it's about identifying the disc,
    // not ripping it. We use the disc name alone (no files yet) to search TMDb.
    if let (Some(disc_name), Some(_searcher)) = (disc_name, state.searcher.as_deref()) {
        let disc_name_clone = disc_name.to_string();
        let state_nc = Arc::clone(state);
        let drive_idx = drive_index;
        tokio::spawn(async move {
            if let Some(ref searcher) = state_nc.searcher {
                // Search TMDb by disc name only (no files available yet)
                let resolved = ripper::resolve_rip_title_with_duration(
                    &disc_name_clone, "", Some(&**searcher),
                ).await;
                if !resolved.0.is_empty() {
                    let year = resolved.1.unwrap_or(0);
                    let mut suggestions = state_nc.naming_suggestions.write().await;
                    suggestions.insert(drive_idx, Some((resolved.0.clone(), year)));
                    drop(suggestions);
                    state_nc.naming_completed.write().await.insert(drive_idx);
                    info!(drive = drive_idx, title = %resolved.0, year, "Auto-naming succeeded from disc insertion");
                    state_nc.sse.send_json(json!({"kind":"rip","action":"named","index":drive_idx,"data":{"title":resolved.0,"year":year,"type":"movie"}}));
                    state_nc.sse.send_json(json!({"kind":"naming","action":"list_updated","data":{}}));
                } else {
                    let mut suggestions = state_nc.naming_suggestions.write().await;
                    suggestions.insert(drive_idx, None);
                    drop(suggestions);
                    state_nc.naming_completed.write().await.insert(drive_idx);
                    state_nc.sse.send_json(json!({"kind":"rip","action":"naming_failed","index":drive_idx,"data":{}}));
                    state_nc.sse.send_json(json!({"kind":"naming","action":"list_updated","data":{}}));
                }
            }
        });
    }

    // If auto-rip is enabled for this drive, start the rip
    if let Ok(drives) = state.autorip_drives.try_read() {
        if drives.contains(&drive_index) {
            info!(drive_index, "Auto-rip triggered for drive");
            let state_clone = Arc::clone(state);
            tokio::spawn(async move {
                ripper::run_rip_task(state_clone, drive_index).await;
            });
        }
    }
}

/// Handle a drive ejection event.
pub async fn handle_drive_ejected(state: &Arc<AppState>, event: &DriveEvent) {
    let drive_index = event.drive_index();
    let device_path = event.device_path();

    info!(drive_index, device_path = %device_path, "Drive ejection event received");

    // Invalidate drives cache
    {
        let mut cache = state.drives_cache.write().await;
        *cache = None;
    }

    // Send SSE notification
    let sse_data = serde_json::json!({
        "type": "drive_ejected",
        "drive_index": drive_index,
        "device_path": device_path,
    });

    state.sse.send_json(sse_data);
}

/// Main event loop for processing drive events.
pub async fn drive_event_loop(state: Arc<AppState>, mut rx: tokio::sync::mpsc::UnboundedReceiver<DriveEvent>) {
    info!("Drive event loop started");

    while let Some(event) = rx.recv().await {
        match &event {
            DriveEvent::Inserted { .. } => {
                handle_drive_inserted(&state, &event).await;
            }
            DriveEvent::Ejected { .. } => {
                handle_drive_ejected(&state, &event).await;
            }
        }
    }

    info!("Drive event loop ended");
}
