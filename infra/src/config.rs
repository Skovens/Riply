use serde::{Deserialize, Serialize};
use tracing::info;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to scan for media files
    pub scan_dir: String,
    /// Path to store movie files
    pub movies_dir: String,
    /// Path to store TV show files
    pub tv_dir: String,
    /// TMDb API key
    pub tmdb_api_key: String,
    /// OMDb API key
    pub omdb_api_key: String,
    /// Database path
    pub db_path: String,
    /// Enable verbose logging
    pub verbose: bool,
    /// Enable AutoRip
    pub autorip_enabled: bool,
}

impl Config {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        info!("Creating default configuration");
        Config {
            scan_dir: String::new(),
            movies_dir: String::new(),
            tv_dir: String::new(),
            tmdb_api_key: String::new(),
            omdb_api_key: String::new(),
            db_path: "media.db".to_string(),
            verbose: false,
            autorip_enabled: false,
        }
    }

    /// Load configuration from a TOML file
    pub fn load(path: &str) -> Result<Self, anyhow::Error> {
        info!("Loading configuration from: {}", path);
        // TODO: Implement TOML file loading
        Ok(Config::new())
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &str) -> Result<(), anyhow::Error> {
        info!("Saving configuration to: {}", path);
        // TODO: Implement TOML file saving
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}