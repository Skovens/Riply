use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use app::JobScheduler;
use infra::drive_events::DriveEventChain;
use infra::{Config, Database};
use riply_core::MetadataSearcher;
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

    info!(addr = %addr, "Starting web server");

    let backend = infra::MakemkvBackend::from_config_or_detect(config.ripping_command())
        .ok()
        .map(|b| Arc::new(b) as Arc<dyn riply_core::RipBackend>);

    let searcher: Option<Arc<dyn MetadataSearcher>> =
        if config.tmdb_api_key.is_some() || config.omdb_api_key.is_some() {
            let client =
                infra::HttpClient::new(config.tmdb_api_key.clone(), config.omdb_api_key.clone());
            Some(Arc::new(client))
        } else {
            None
        };

    // Build event-driven drive detection chain and select source
    let chain = DriveEventChain::new();
    let selected = chain.select();

    let (drive_event_source, event_source_available, drive_event_rx) = if let Some(source) = selected.source {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let source_arc = Arc::clone(&source);
        tokio::spawn(async move {
            source_arc.listen(tx);
        });
        (Some(source), true, Some(rx))
    } else {
        (None, false, None)
    };

    let state = Arc::new(AppState {
        db: Arc::new(db),
        config: Arc::new(RwLock::new(config.clone())),
        config_path,
        sse: SseManager::new(256),
        backend,
        searcher,
        scheduler: JobScheduler::new(),
        active_rips: RwLock::new(HashMap::new()),
        autorip_drives: RwLock::new(HashSet::new()),
        autorip_handle: RwLock::new(None),
        healthchecks_handle: RwLock::new(None),
        metrics: web::metrics::Metrics::default(),
        disc_names: RwLock::new(HashMap::new()),
        completed_rips: RwLock::new(HashSet::new()),
        naming_completed: RwLock::new(HashSet::new()),
        naming_suggestions: RwLock::new(HashMap::new()),
        pending_accepts: RwLock::new(HashMap::new()),
        drive_event_source,
        drive_event_rx: RwLock::new(drive_event_rx),
        event_source_available,
    });

    // Start the auto-rip monitor. It watches drives that have auto-rip enabled
    // per drive and does no work otherwise.
    let state_clone = Arc::clone(&state);
    let handle = tokio::spawn(async move {
        web::autorip_poll_loop(state_clone).await;
    });
    *state.autorip_handle.write().await = Some(handle);
    info!("Auto-rip monitor started");

    // Start healthchecks.io heartbeat if configured
    if let Some(hc) = infra::healthchecks::HealthchecksClient::from_config(&config.healthchecks) {
        hc.start().await;
        let interval =
            std::time::Duration::from_secs(config.healthchecks.ping_interval_minutes * 60);
        let interval_mins = config.healthchecks.ping_interval_minutes;
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tokio::spawn(async move {
            info!(
                interval_min = interval_mins,
                "Healthchecks heartbeat started"
            );
            loop {
                timer.tick().await;
                hc.ping().await;
            }
        });
    }

    let router = web::create_router(Arc::clone(&state));

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(addr = %addr, "Listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Stop the auto-rip monitor so the process exits promptly
    if let Some(handle) = state.autorip_handle.write().await.take() {
        handle.abort();
    }

    // Log metadata API usage stats on shutdown
    if let Some(searcher) = &state.searcher {
        searcher.log_stats().await;
    }

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
