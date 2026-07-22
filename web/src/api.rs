use axum::{
    response::sse::{Event, Sse},
    routing::{get, post},
    Router,
};
use tracing::info;

/// Create the API router with all endpoints
pub fn create_router() -> Router {
    info!("Creating API router");

    Router::new()
        // Health check
        .route("/health", get(health_check))
        // Files endpoints
        .route("/api/files", get(list_files))
        .route("/api/files/{id}", get(get_file))
        .route("/api/files/{id}", post(update_file))
        // Review endpoints
        .route("/api/review", get(get_review_queue))
        .route("/api/review/{id}/accept", post(accept_file))
        .route("/api/review/{id}/reject", post(reject_file))
        // Mover endpoints
        .route("/api/move", post(start_move))
        .route("/api/move/status", get(get_move_status))
        // Ripper endpoints
        .route("/api/ripper/drives", get(list_drives))
        .route("/api/ripper/{index}/start", post(start_rip))
        .route("/api/ripper/{index}/cancel", post(cancel_rip))
        // SSE endpoint for real-time updates
        .route("/api/sse", get(sse_endpoint))
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// List files endpoint
async fn list_files() -> String {
    info!("Listing files");
    "[]".to_string()
}

/// Get file endpoint
async fn get_file(axum::extract::Path(id): axum::extract::Path<String>) -> String {
    info!("Getting file: {}", id);
    format!("{{\"id\": \"{}\"}}", id)
}

/// Update file endpoint
async fn update_file(axum::extract::Path(id): axum::extract::Path<String>) -> String {
    info!("Updating file: {}", id);
    format!("{{\"id\": \"{}\", \"status\": \"updated\"}}", id)
}

/// Get review queue endpoint
async fn get_review_queue() -> String {
    info!("Getting review queue");
    "[]".to_string()
}

/// Accept file endpoint
async fn accept_file(axum::extract::Path(id): axum::extract::Path<String>) -> String {
    info!("Accepting file: {}", id);
    format!("{{\"id\": \"{}\", \"status\": \"confirmed\"}}", id)
}

/// Reject file endpoint
async fn reject_file(axum::extract::Path(id): axum::extract::Path<String>) -> String {
    info!("Rejecting file: {}", id);
    format!("{{\"id\": \"{}\", \"status\": \"rejected\"}}", id)
}

/// Start move endpoint
async fn start_move() -> String {
    info!("Starting move");
    "{ \"status\": \"started\" }".to_string()
}

/// Get move status endpoint
async fn get_move_status() -> String {
    info!("Getting move status");
    "{ \"status\": \"running\", \"progress\": 0.0 }".to_string()
}

/// List drives endpoint
async fn list_drives() -> String {
    info!("Listing drives");
    "[]".to_string()
}

/// Start rip endpoint
async fn start_rip(axum::extract::Path(index): axum::extract::Path<usize>) -> String {
    info!("Starting rip on drive: {}", index);
    format!("{{\"drive_index\": {}, \"status\": \"ripping\"}}", index)
}

/// Cancel rip endpoint
async fn cancel_rip(axum::extract::Path(index): axum::extract::Path<usize>) -> String {
    info!("Cancelling rip on drive: {}", index);
    format!("{{\"drive_index\": {}, \"status\": \"cancelled\"}}", index)
}

/// SSE endpoint for real-time updates
async fn sse_endpoint(
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_stream::StreamExt;

    info!("SSE endpoint connected");

    let (tx, rx) = mpsc::channel(32);

    // Send initial event
    tokio::spawn(async move {
        let _ = tx.send(Event::default().event("connected")).await;
    });

    let stream = ReceiverStream::new(rx).map(Ok);
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}
