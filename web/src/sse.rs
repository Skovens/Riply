use axum::response::sse::Event;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// SSE manager for broadcasting progress updates to all connected clients.
///
/// All messages are structured JSON with the envelope:
/// `{"kind":"...","action":"...","index":N,"data":{...}}`
#[derive(Clone)]
pub struct SseManager {
    tx: broadcast::Sender<String>,
}

impl SseManager {
    /// Create a new SSE manager with a broadcast channel
    pub fn new(capacity: usize) -> Self {
        info!(capacity, "Creating SSE manager");
        let (tx, _) = broadcast::channel(capacity);
        SseManager { tx }
    }

    /// Subscribe to SSE events (returns a receiver for new connections)
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Broadcast a structured JSON event.
    pub fn send_json(&self, event: Value) {
        if let Err(broadcast::error::SendError(_)) = self.tx.send(event.to_string()) {
            warn!("SSE event dropped (no active receivers)");
        }
    }

    /// Convert a raw broadcast message into an SSE Event.
    pub fn message_to_event(msg: &str) -> Event {
        Event::default().event("status").data(msg)
    }
}
