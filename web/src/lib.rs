// Web layer - HTTP API and SSE endpoints
pub mod api;
pub mod sse;

pub use api::{create_router, AppState};
pub use sse::SseManager;
