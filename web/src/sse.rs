use axum::response::sse::Event;
use tokio::sync::broadcast;
use tracing::info;

/// SSE manager for broadcasting progress updates to all connected clients.
///
/// All messages use a consistent format: `"event:{name}\ndata:{payload}"`
/// for easy parsing on the client side.
#[derive(Clone)]
pub struct SseManager {
    tx: broadcast::Sender<String>,
}

impl SseManager {
    /// Create a new SSE manager with a broadcast channel
    pub fn new(capacity: usize) -> Self {
        info!("Creating SSE manager with capacity {}", capacity);
        let (tx, _) = broadcast::channel(capacity);
        SseManager { tx }
    }

    /// Subscribe to SSE events (returns a receiver for new connections)
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Broadcast a status update event (e.g. "scan:progress:5:10")
    pub fn send_status(&self, status: &str) {
        let _ = self.tx.send(format!("event:status\ndata:{}", status));
    }

    /// Broadcast a progress update (alias for send_status with "progress" prefix)
    pub fn send_progress(&self, message: &str) {
        let _ = self.tx.send(format!("event:progress\ndata:{}", message));
    }

    /// Broadcast a log message (e.g. "warn:Failed to save file")
    pub fn send_log(&self, level: &str, message: &str) {
        let _ = self
            .tx
            .send(format!("event:log\ndata:{}:{}", level, message));
    }

    /// Convert a raw broadcast message into an SSE Event.
    ///
    /// Expected format: `"event:{name}\ndata:{payload}"` or `"data:{payload}"`
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
