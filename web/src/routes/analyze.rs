use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};
use serde_json::json;
use tracing::info;

use crate::api::{ApiResponse, AppState, JobStartedResponse};
use crate::error::WebError;

/// Router for the analyze domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/analyze", post(start_analyze))
        .route("/v1/analyze/retry-failed", post(retry_failed))
        .with_state(state)
}

pub async fn start_analyze(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<JobStartedResponse>>, WebError> {
    let db = state.db.clone();
    let sse = state.sse.clone();
    let config = state.config.read().await.clone();

    let job_id = state
        .scheduler
        .start_analyze_job(move |cancel, progress| {
            let db = db.clone();
            let sse = sse.clone();
            async move {
                let records = db.get_files_by_status("Scanned").await?;
                if records.is_empty() {
                    sse.send_json(json!({"kind":"analyze","action":"complete","index":0,"data":{"job_id":"analyze","matched":0,"review":0}}));
                    return Ok(());
                }

                progress.set_total(records.len());
                sse.send_json(json!({"kind":"analyze","action":"starting","index":0,"data":{"total":records.len(),"job_id":"analyze"}}));

                let searcher: Option<Box<dyn riply_core::MetadataSearcher>> =
                    if config.tmdb_api_key.is_some() || config.omdb_api_key.is_some() {
                        let client = infra::HttpClient::new(
                            config.tmdb_api_key.clone(),
                            config.omdb_api_key.clone(),
                        );
                        Some(Box::new(client))
                    } else {
                        None
                    };

                let analyzer = app::AnalyzerService::new(searcher);
                let mut matched = 0;
                let mut review = 0;

                for (i, record) in records.iter().enumerate() {
                    if cancel.is_cancelled() {
                        sse.send_json(json!({"kind":"analyze","action":"cancelled","index":0,"data":{"job_id":"analyze"}}));
                        return Ok(());
                    }
                    match analyzer.analyze_file(record).await {
                        Ok(updated) => {
                            match updated.status() {
                                riply_core::PipelineStatus::Matched => matched += 1,
                                riply_core::PipelineStatus::NeedsReview => review += 1,
                                _ => {}
                            }
                            let _ = db.update_file(&updated).await;
                        }
                        Err(e) => {
                            sse.send_log(
                                "warn",
                                &format!("Failed to analyze {}: {}", record.filename(), e),
                            );
                        }
                    }
                    progress.increment();
                    if (i + 1) % 10 == 0 || i + 1 == records.len() {
                        sse.send_json(json!({"kind":"analyze","action":"progress","index":0,"data":{"job_id":"analyze","processed":i+1,"total":records.len()}}));
                    }
                }

                sse.send_json(json!({"kind":"analyze","action":"complete","index":0,"data":{"job_id":"analyze","matched":matched,"review":review}}));
                info!(matched, review, "Analysis complete");
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

pub async fn retry_failed(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<JobStartedResponse>>, WebError> {
    let db = state.db.clone();
    let sse = state.sse.clone();
    let config = state.config.read().await.clone();

    let failed_files = db
        .get_files_by_status("Failed")
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;

    for file in &failed_files {
        let mut updated = file.clone();
        updated.update_status(riply_core::PipelineStatus::Scanned);
        let _ = db.update_file(&updated).await;
    }

    let m = config.tmdb_api_key.clone();
    let o = config.omdb_api_key.clone();

    let job_id = state
        .scheduler
        .start_analyze_job(move |cancel, progress| {
            let db = db.clone();
            let sse = sse.clone();
            async move {
                let records = db.get_files_by_status("Scanned").await?;
                if records.is_empty() {
                    sse.send_json(json!({"kind":"analyze","action":"complete","index":0,"data":{"job_id":"analyze","matched":0,"review":0}}));
                    return Ok(());
                }

                progress.set_total(records.len());
                sse.send_json(json!({"kind":"analyze","action":"starting","index":0,"data":{"total":records.len(),"job_id":"analyze"}}));

                let searcher: Option<Box<dyn riply_core::MetadataSearcher>> =
                    if m.is_some() || o.is_some() {
                        let client = infra::HttpClient::new(m, o);
                        Some(Box::new(client))
                    } else {
                        None
                    };

                let analyzer = app::AnalyzerService::new(searcher);
                let mut matched = 0;
                let mut review = 0;

                for (i, record) in records.iter().enumerate() {
                    if cancel.is_cancelled() {
                        sse.send_json(json!({"kind":"analyze","action":"cancelled","index":0,"data":{"job_id":"analyze"}}));
                        return Ok(());
                    }
                    match analyzer.analyze_file(record).await {
                        Ok(updated) => {
                            match updated.status() {
                                riply_core::PipelineStatus::Matched => matched += 1,
                                riply_core::PipelineStatus::NeedsReview => review += 1,
                                _ => {}
                            }
                            let _ = db.update_file(&updated).await;
                        }
                        Err(e) => {
                            sse.send_log(
                                "warn",
                                &format!("Failed to analyze {}: {}", record.filename(), e),
                            );
                        }
                    }

                    progress.increment();
                    if i % 5 == 0 {
                        sse.send_json(json!({"kind":"analyze","action":"progress","index":0,"data":{"job_id":"analyze","processed":i+1,"total":records.len()}}));
                    }
                }

                sse.send_json(json!({"kind":"analyze","action":"complete","index":0,"data":{"job_id":"analyze","matched":matched,"review":review}}));
                info!(matched, review, "Analysis complete (retry)");
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
