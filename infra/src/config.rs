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
}

impl RippingConfig {
    /// Resolve the command to use. Priority: `command` > `makemkvcon_path` (deprecated).
    pub fn resolved_command(&self) -> Option<&str> {
        self.command
            .as_deref()
            .or(self.makemkvcon_path.as_deref())
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
            verbose: false,
            tmdb_api_key: None,
            omdb_api_key: None,
            makemkvcon_path: None,
            ripping: RippingConfig::default(),
            host: default_host(),
            port: default_port(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &str) -> Result<Self, anyhow::Error> {
        info!("Loading configuration from: {}", path);

        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;

        info!("Configuration loaded successfully");
        Ok(config)
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &str) -> Result<(), anyhow::Error> {
        info!("Saving configuration to: {}", path);

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
            verbose: true,
            tmdb_api_key: None,
            omdb_api_key: None,
            makemkvcon_path: None,
            ripping: RippingConfig::default(),
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
        };
        // command wins over makemkvcon_path
        assert_eq!(
            c.resolved_command(),
            Some("flatpak run --command=makemkvcon com.makemkv.MakeMKV")
        );

        let c = RippingConfig {
            command: None,
            makemkvcon_path: Some("/usr/bin/makemkvcon".to_string()),
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
        assert_eq!(config.makemkvcon_path.as_deref(), Some("/usr/bin/makemkvcon"));
        assert_eq!(config.ripping_command(), Some("/usr/bin/makemkvcon"));
    }

    #[test]
    fn test_ripping_command_priority() {
        let config = Config {
            makemkvcon_path: Some("/old/path".to_string()),
            ripping: RippingConfig {
                command: Some("flatpak run --command=makemkvcon com.makemkv.MakeMKV".to_string()),
                makemkvcon_path: None,
            },
            ..Config::default()
        };
        // ripping.command wins over makemkvcon_path
        assert_eq!(
            config.ripping_command(),
            Some("flatpak run --command=makemkvcon com.makemkv.MakeMKV")
        );
    }
}
