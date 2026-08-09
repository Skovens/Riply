// ADR-006: HTTP Framework — Axum router and state management
// ADR-011: Real-Time Updates — SSE endpoint and manager

// Web layer - HTTP API and SSE endpoints
pub mod api;
pub mod error;
pub mod routes;
pub mod sse;

pub use api::{create_router, AppState};
pub use routes::ripper::autorip_poll_loop;
pub use sse::SseManager;
