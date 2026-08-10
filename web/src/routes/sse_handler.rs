use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use tracing::info;

use crate::api::AppState;
use crate::sse::SseManager;

/// Router for the SSE domain
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/sse", get(sse_endpoint))
        .with_state(state)
}

pub async fn sse_endpoint(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    info!("SSE endpoint connected");

    let mut rx = state.sse.subscribe();

    let stream = async_stream::stream! {
        yield Ok(Event::default().event("connected").data("SSE connection established"));
        while let Ok(msg) = rx.recv().await {
            yield Ok(SseManager::message_to_event(&msg));
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}
