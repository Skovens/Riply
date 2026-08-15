use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;
use tracing::info;

use crate::api::{ApiResponse, AppState, JobStartedResponse};
use crate::error::WebError;

/// Router for the scan domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/scan", post(start_scan))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct ScanBody {
    pub directory: String,
}

pub async fn start_scan(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ScanBody>,
) -> Result<Json<ApiResponse<JobStartedResponse>>, WebError> {
    let dir = PathBuf::from(&body.directory);
    if !dir.exists() {
        return Err(WebError::BadRequest(format!(
            "Directory does not exist: {}",
            body.directory
        )));
    }

    let db = state.db.clone();
    let sse = state.sse.clone();
    let searcher = state.searcher.clone();
    let directory = body.directory.clone();

    let job_id = state
        .scheduler
        .start_scan_job(&body.directory, move |cancel, progress| {
            let db = db.clone();
            let sse = sse.clone();
            let searcher = searcher.clone();
            let directory = directory.clone();
            async move {
                let scanner = app::ScannerService::new();
                let result = scanner.scan_directory(&dir, searcher.as_deref()).await?;
                let records = result.records;
                let groups = result.title_groups;

                // Save title groups
                for group in &groups {
                    if let Err(e) = db.save_title_group(group).await {
                        sse.send_log("warn", &format!("Failed to save title group: {}", e));
                    }
                }

                let mut duplicates = HashMap::new();
                for record in &records {
                    if let Ok(Some(existing)) = db.find_file_by_hash(&record.hash()).await {
                        if existing.path() != record.path() {
                            duplicates.insert(record.hash().to_string(), existing.path().clone());
                        }
                    }
                }

                progress.set_total(records.len());

                sse.send_status(&format!("scan:starting:{}", records.len()));

                for (i, record) in records.iter().enumerate() {
                    if cancel.is_cancelled() {
                        sse.send_status("scan:cancelled");
                        return Ok(());
                    }

                    let mut save_record = record.clone();
                    if let Some(dup_path) = duplicates.get(record.hash()) {
                        *save_record.duplicate_path_mut() = Some(dup_path.clone());
                        sse.send_log(
                            "info",
                            &format!(
                                "Duplicate found: {} matches {}",
                                record.filename(),
                                dup_path.display()
                            ),
                        );
                    }

                    if let Err(e) = db.save_file(&save_record).await {
                        sse.send_log(
                            "warn",
                            &format!("Failed to save {}: {}", record.filename(), e),
                        );
                    }
                    progress.increment();
                    if (i + 1) % 10 == 0 || i + 1 == records.len() {
                        sse.send_status(&format!("scan:progress:{}:{}", i + 1, records.len()));
                    }
                }

                sse.send_status(&format!("scan:complete:{}", records.len()));
                info!(count = records.len(), dir = %directory, "Scan complete");
                Ok(())
            }
        })
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(JobStartedResponse {
        job_id,
        status: "Queued".to_string(),
    })))
}
