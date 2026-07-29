use std::collections::HashMap;
use std::sync::Arc;

use app::JobScheduler;
use infra::{Config, Database};
use tokio::sync::RwLock;
use tracing::info;
use web::{AppState, SseManager};

/// Start the web API server
pub async fn serve(
    db: Database,
    config: Config,
    config_path: Option<String>,
    host: Option<String>,
    port: Option<u16>,
) -> Result<(), anyhow::Error> {
    let host = host.unwrap_or_else(|| config.host.clone());
    let port = port.unwrap_or(config.port);
    let addr = format!("{}:{}", host, port);

    info!("Starting web server on {}", addr);

    let backend = infra::MakemkvBackend::from_config_or_detect(config.ripping_command())
        .ok()
        .map(|b| Arc::new(b) as Arc<dyn riply_core::RipBackend>);

    let state = Arc::new(AppState {
        db: Arc::new(db),
        config: Arc::new(RwLock::new(config.clone())),
        config_path,
        sse: SseManager::new(256),
        backend,
        scheduler: JobScheduler::new(),
        active_rips: RwLock::new(HashMap::new()),
        autorip_handle: RwLock::new(None),
        healthchecks_handle: RwLock::new(None),
    });

    // Start autorip if enabled
    if config.ripping.autorip_enabled {
        let state_clone = Arc::clone(&state);
        let handle = tokio::spawn(async move {
            web::autorip_poll_loop(state_clone).await;
        });
        *state.autorip_handle.write().await = Some(handle);
        info!("Auto-rip enabled — polling for disc insertion");
    }

    // Start healthchecks.io heartbeat if configured
    if let Some(hc) = infra::healthchecks::HealthchecksClient::from_config(&config.healthchecks) {
        hc.start().await;
        let interval =
            std::time::Duration::from_secs(config.healthchecks.ping_interval_minutes * 60);
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tokio::spawn(async move {
            info!(
                "healthchecks: heartbeat started (every {} min)",
                config.healthchecks.ping_interval_minutes
            );
            loop {
                timer.tick().await;
                hc.ping().await;
            }
        });
    }

    let router = web::create_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on {}", addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
