//! Event handler for drive events.
//!
//! Receives events from the event source channel, processes them,
//! updates drive state, and sends SSE notifications.

use std::sync::Arc;

use infra::drive_events::DriveEvent;
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

    // Send SSE notification
    let sse_data = serde_json::json!({
        "type": "drive_inserted",
        "drive_index": drive_index,
        "device_path": device_path,
        "disc_name": disc_name,
    });

    state.sse.send_json(sse_data);

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
