use std::sync::Arc;

use infra::{Config, Database};
use tokio::sync::RwLock;
use tracing::info;
use web::{AppState, SseManager};

/// Start the web API server
pub async fn serve(
    db: Database,
    config: Config,
    host: Option<String>,
    port: Option<u16>,
) -> Result<(), anyhow::Error> {
    let host = host.unwrap_or_else(|| config.host.clone());
    let port = port.unwrap_or(config.port);
    let addr = format!("{}:{}", host, port);

    info!("Starting web server on {}", addr);

    let state = Arc::new(AppState {
        db: Arc::new(db),
        config: Arc::new(RwLock::new(config)),
        sse: SseManager::new(256),
    });

    let router = web::create_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on {}", addr);

    axum::serve(listener, router).await?;

    Ok(())
}
