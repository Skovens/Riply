use axum::response::sse::{Event, Sse};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;

/// SSE manager for broadcasting progress updates
pub struct SseManager {
    tx: mpsc::Sender<Event>,
}

impl SseManager {
    /// Create a new SSE manager
    pub fn new() -> (Self, ReceiverStream<Event>) {
        info!("Creating SSE manager");
        let (tx, rx) = mpsc::channel(32);
        (
            SseManager { tx },
            ReceiverStream::new(rx),
        )
    }

    /// Send a progress update
    pub async fn send_progress(&self, message: &str) -> Result<(), mpsc::error::SendError<Event>> {
        info!("Sending SSE progress: {}", message);
        self.tx.send(Event::default().data(message)).await
    }

    /// Send a status update
    pub async fn send_status(&self, status: &str) -> Result<(), mpsc::error::SendError<Event>> {
        info!("Sending SSE status: {}", status);
        self.tx.send(Event::default().event("status").data(status)).await
    }

    /// Send a log message
    pub async fn send_log(&self, level: &str, message: &str) -> Result<(), mpsc::error::SendError<Event>> {
        info!("Sending SSE log: [{}] {}", level, message);
        self.tx.send(Event::default().event("log").data(format!("{}:{}", level, message))).await
    }
}