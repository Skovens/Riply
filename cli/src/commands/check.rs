use reqwest::Client;
use serde::Deserialize;
use tracing::info;

/// Health check response from /health/detailed
#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
    database: String,
    drives: DriveHealth,
    makemkv: MakeMkvHealth,
    tmdb: TmdbHealth,
}

#[derive(Debug, Deserialize)]
struct DriveHealth {
    count: usize,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MakeMkvHealth {
    available: bool,
    #[allow(dead_code)]
    status: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbHealth {
    configured: bool,
    #[allow(dead_code)]
    status: String,
    error: Option<String>,
}

/// Run a comprehensive health check against the server
pub async fn check_server(url: &str) -> anyhow::Result<()> {
    let client = Client::new();
    
    info!("Checking server at {}", url);
    
    // Check /health
    let health_url = format!("{}/health", url);
    match client.get(&health_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                info!("✅ /health endpoint responding");
            } else {
                info!("⚠️ /health returned status: {}", resp.status());
            }
        }
        Err(e) => {
            anyhow::bail!("❌ Cannot reach /health: {}", e);
        }
    }
    
    // Check /health/detailed
    let detailed_url = format!("{}/health/detailed", url);
    match client.get(&detailed_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                let body: HealthResponse = resp.json().await?;
                
                info!("📊 Health Check Results:");
                info!("  Status: {}", body.status);
                info!("  Version: {}", body.version);
                info!("  Database: {}", body.database);
                info!("  Drives: {} (status: {})", body.drives.count, body.drives.status);
                info!("  MakeMKV: {}", if body.makemkv.available { "available" } else { "unavailable" });
                info!("  TMDb: {}", if body.tmdb.configured { "configured" } else { "not configured" });
                
                if let Some(ref err) = body.drives.error {
                    info!("  ⚠️ Drive error: {}", err);
                }
                if let Some(ref err) = body.makemkv.error {
                    info!("  ⚠️ MakeMKV error: {}", err);
                }
                if let Some(ref err) = body.tmdb.error {
                    info!("  ℹ️  {}", err);
                }
                
                // Overall status
                match body.status.as_str() {
                    "ok" => info!("✅ Server is fully operational"),
                    "degraded" => info!("⚠️ Server is operational but degraded"),
                    "error" => info!("❌ Server has errors"),
                    _ => info!("ℹ️  Server status: {}", body.status),
                }
            } else {
                info!("⚠️ /health/detailed returned status: {}", resp.status());
            }
        }
        Err(e) => {
            anyhow::bail!("❌ Cannot reach /health/detailed: {}", e);
        }
    }
    
    Ok(())
}

/// Check local server health (default to localhost:3000)
pub async fn check_local() -> anyhow::Result<()> {
    check_server("http://localhost:3000").await
}
