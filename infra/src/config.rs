use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

/// Ripping backend configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RippingConfig {
    /// Command to run for ripping. Can be:
    /// - "makemkvcon" (native install)
    /// - "flatpak run --command=makemkvcon com.makemkv.MakeMKV" (Flatpak)
    ///
    /// Auto-detected if not set.
    #[serde(default)]
    pub command: Option<String>,

    /// Path to makemkvcon binary (deprecated, use `command` instead).
    /// Still loaded for backward compatibility — maps to `command`.
    #[serde(default)]
    pub makemkvcon_path: Option<String>,

    /// Eject disc after auto-rip completes. Default: true.
    #[serde(default = "default_true")]
    pub autorip_eject_after: bool,
}

/// healthchecks.io integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthchecksConfig {
    /// UUID from healthchecks.io. Enables heartbeat pings when set.
    #[serde(default)]
    pub uuid: String,
    /// Ping interval in minutes (default: 5).
    #[serde(default = "default_ping_interval")]
    pub ping_interval_minutes: u64,
}

impl Default for HealthchecksConfig {
    fn default() -> Self {
        Self {
            uuid: String::new(),
            ping_interval_minutes: 5,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_ping_interval() -> u64 {
    5
}

impl RippingConfig {
    /// Resolve the command to use. Priority: `command` > `makemkvcon_path` (deprecated).
    pub fn resolved_command(&self) -> Option<&str> {
        self.command.as_deref().or(self.makemkvcon_path.as_deref())
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the SQLite database file
    pub database_path: String,
    /// Directory to scan for media files
    pub scan_directory: Option<String>,
    /// Movies destination directory
    pub movies_directory: String,
    /// TV Shows destination directory
    pub tv_directory: String,
    /// Directory to rip disc content into
    pub rip_output_directory: Option<String>,
    /// Enable verbose logging
    pub verbose: bool,
    /// TMDb API key (optional, enables metadata lookups)
    pub tmdb_api_key: Option<String>,
    /// OMDb API key (optional, fallback metadata source)
    pub omdb_api_key: Option<String>,
    /// Path to makemkvcon binary (deprecated, use `ripping.command` instead)
    #[serde(default)]
    pub makemkvcon_path: Option<String>,
    /// Ripping backend configuration
    #[serde(default)]
    pub ripping: RippingConfig,
    /// healthchecks.io integration configuration
    #[serde(default)]
    pub healthchecks: HealthchecksConfig,
    /// Web server bind address
    #[serde(default = "default_host")]
    pub host: String,
    /// Web server bind port
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_path: "riply.db".to_string(),
            scan_directory: None,
            movies_directory: "movies".to_string(),
            tv_directory: "tv".to_string(),
            rip_output_directory: None,
            verbose: false,
            tmdb_api_key: None,
            omdb_api_key: None,
            makemkvcon_path: None,
            ripping: RippingConfig::default(),
            healthchecks: HealthchecksConfig::default(),
            host: default_host(),
            port: default_port(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &str) -> Result<Self, anyhow::Error> {
        info!(path = %path, "Loading configuration");

        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;

        info!("Configuration loaded successfully");
        Ok(config)
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &str) -> Result<(), anyhow::Error> {
        info!(path = %path, "Saving configuration");

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;

        info!("Configuration saved successfully");
        Ok(())
    }

    /// Get the database path as a PathBuf
    pub fn database_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.database_path)
    }

    /// Get the movies directory as a PathBuf
    pub fn movies_directory_buf(&self) -> PathBuf {
        PathBuf::from(&self.movies_directory)
    }

    /// Get the TV directory as a PathBuf
    pub fn tv_directory_buf(&self) -> PathBuf {
        PathBuf::from(&self.tv_directory)
    }

    /// Create a default configuration file if it doesn't exist
    pub fn create_default(path: &str) -> Result<(), anyhow::Error> {
        let config = Self::default();
        config.save(path)
    }

    /// Resolve the ripping command from config.
    /// Priority: `ripping.command` > `makemkvcon_path` (deprecated).
    pub fn ripping_command(&self) -> Option<&str> {
        self.ripping
            .resolved_command()
            .or(self.makemkvcon_path.as_deref())
    }

    /// Validate that the config has the required directories for ripping.
    /// Returns `Ok(())` if valid, or a descriptive error message.
    pub fn validate_for_ripping(&self) -> Result<(), String> {
        match &self.rip_output_directory {
            Some(dir) if !dir.is_empty() => {
                let path = std::path::Path::new(dir);
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() && !parent.exists() {
                        return Err(format!(
                            "Parent directory of rip_output_directory does not exist: {}",
                            parent.display()
                        ));
                    }
                }
                Ok(())
            }
            _ => Err(
                "rip_output_directory is not set. Go to Settings → Ripping to configure it."
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.database_path, "riply.db");
        assert_eq!(config.movies_directory, "movies");
        assert_eq!(config.tv_directory, "tv");
        assert!(!config.verbose);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 3000);
    }

    #[test]
    fn test_save_and_load_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let config = Config {
            database_path: "test.db".to_string(),
            scan_directory: Some("/test".to_string()),
            movies_directory: "movies".to_string(),
            tv_directory: "tv".to_string(),
            rip_output_directory: Some("/rips".to_string()),
            verbose: true,
            tmdb_api_key: None,
            omdb_api_key: None,
            makemkvcon_path: None,
            ripping: RippingConfig::default(),
            healthchecks: HealthchecksConfig::default(),
            host: "127.0.0.1".to_string(),
            port: 8080,
        };

        // Save config
        config.save(config_path.to_str().unwrap()).unwrap();

        // Load config
        let loaded = Config::load(config_path.to_str().unwrap()).unwrap();

        assert_eq!(loaded.database_path, "test.db");
        assert_eq!(loaded.scan_directory, Some("/test".to_string()));
        assert_eq!(loaded.movies_directory, "movies");
        assert_eq!(loaded.tv_directory, "tv");
        assert_eq!(loaded.rip_output_directory, Some("/rips".to_string()));
        assert!(loaded.verbose);
        assert_eq!(loaded.host, "127.0.0.1");
        assert_eq!(loaded.port, 8080);
    }

    #[test]
    fn test_create_default_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        Config::create_default(config_path.to_str().unwrap()).unwrap();

        // Verify file was created
        assert!(config_path.exists());

        // Verify we can load it
        let config = Config::load(config_path.to_str().unwrap()).unwrap();
        assert_eq!(config.database_path, "riply.db");
    }

    #[test]
    fn test_ripping_config_resolved_command() {
        let c = RippingConfig::default();
        assert_eq!(c.resolved_command(), None);

        let c = RippingConfig {
            command: Some("flatpak run --command=makemkvcon com.makemkv.MakeMKV".to_string()),
            makemkvcon_path: Some("/usr/bin/makemkvcon".to_string()),
            ..Default::default()
        };
        // command wins over makemkvcon_path
        assert_eq!(
            c.resolved_command(),
            Some("flatpak run --command=makemkvcon com.makemkv.MakeMKV")
        );

        let c = RippingConfig {
            command: None,
            makemkvcon_path: Some("/usr/bin/makemkvcon".to_string()),
            ..Default::default()
        };
        assert_eq!(c.resolved_command(), Some("/usr/bin/makemkvcon"));
    }

    #[test]
    fn test_config_backward_compat() {
        let toml = r#"
database_path = "test.db"
movies_directory = "movies"
tv_directory = "tv"
verbose = false
makemkvcon_path = "/usr/bin/makemkvcon"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(
            config.makemkvcon_path.as_deref(),
            Some("/usr/bin/makemkvcon")
        );
        assert_eq!(config.ripping_command(), Some("/usr/bin/makemkvcon"));
    }

    #[test]
    fn test_ripping_command_priority() {
        let config = Config {
            makemkvcon_path: Some("/old/path".to_string()),
            ripping: RippingConfig {
                command: Some("flatpak run --command=makemkvcon com.makemkv.MakeMKV".to_string()),
                makemkvcon_path: None,
                ..Default::default()
            },
            ..Config::default()
        };
        // ripping.command wins over makemkvcon_path
        assert_eq!(
            config.ripping_command(),
            Some("flatpak run --command=makemkvcon com.makemkv.MakeMKV")
        );
    }

    #[test]
    fn test_validate_for_ripping_missing() {
        let config = Config::default();
        let err = config.validate_for_ripping().unwrap_err();
        assert!(err.contains("rip_output_directory is not set"));
    }

    #[test]
    fn test_validate_for_ripping_empty_string() {
        let config = Config {
            rip_output_directory: Some("".to_string()),
            ..Config::default()
        };
        let err = config.validate_for_ripping().unwrap_err();
        assert!(err.contains("rip_output_directory is not set"));
    }

    #[test]
    fn test_validate_for_ripping_valid() {
        let dir = tempdir().unwrap();
        let config = Config {
            rip_output_directory: Some(dir.path().to_string_lossy().to_string()),
            ..Config::default()
        };
        assert!(config.validate_for_ripping().is_ok());
    }

    #[test]
    fn test_validate_for_ripping_nonexistent_parent() {
        let config = Config {
            rip_output_directory: Some("/nonexistent/path/rips".to_string()),
            ..Config::default()
        };
        let err = config.validate_for_ripping().unwrap_err();
        assert!(err.contains("Parent directory"));
    }
}
