use axum::response::sse::Event;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::info;

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
    /// The JSON must include "kind", "action", "index", and "data" fields.
    pub fn send_json(&self, event: Value) {
        let _ = self.tx.send(event.to_string());
    }

    /// Broadcast a status update event (legacy — use send_json instead).
    /// Kept for backward compatibility during transition.
    pub fn send_status(&self, status: &str) {
        let _ = self.tx.send(format!("event:status\ndata:{}", status));
    }

    /// Broadcast a progress update (alias for send_status with "progress" prefix).
    /// Kept for backward compatibility during transition.
    pub fn send_progress(&self, message: &str) {
        let _ = self.tx.send(format!("event:progress\ndata:{}", message));
    }

    /// Broadcast a log message.
    /// Kept for backward compatibility during transition.
    pub fn send_log(&self, level: &str, message: &str) {
        let _ = self
            .tx
            .send(format!("event:log\ndata:{}:{}", level, message));
    }

    /// Convert a raw broadcast message into an SSE Event.
    ///
    /// Supports both legacy format (`"event:{name}\ndata:{payload}"`)
    /// and JSON format (`{"kind":"...","action":"..."}`).
    pub fn message_to_event(msg: &str) -> Event {
        if let Some((header, body)) = msg.split_once('\n') {
            if let Some(event_name) = header.strip_prefix("event:") {
                Event::default()
                    .event(event_name)
                    .data(body.strip_prefix("data:").unwrap_or(body))
            } else {
                Event::default().data(msg)
            }
        } else if let Some(body) = msg.strip_prefix("data:") {
            Event::default().data(body)
        } else {
            Event::default().data(msg)
        }
    }
}
