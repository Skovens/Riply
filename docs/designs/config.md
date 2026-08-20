# Design: Configuration System

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Load, save, and manage application configuration from a TOML file. This document describes the *current* behavior of the config system.

---

## Config File Format

### `config.toml`

```toml
database_path = "riply.db"
scan_directory = "/path/to/scan"
movies_directory = "movies"
tv_directory = "tv"
rip_output_directory = "/path/to/rips"
verbose = false
tmdb_api_key = "your-tmdb-key"
omdb_api_key = "your-omdb-key"
makemkvcon_path = "/usr/bin/makemkvcon"  # deprecated, use ripping.command

[ripping]
command = "flatpak run --command=makemkvcon com.makemkv.MakeMKV"
autorip_eject_after = true

[healthchecks]
uuid = "your-healthchecks-uuid"
ping_interval_minutes = 5

host = "0.0.0.0"
port = 3000
```

---

## Config Struct

### `Config`

```rust
pub struct Config {
    pub database_path: String,           // SQLite DB path
    pub scan_directory: Option<String>,  // Default scan directory
    pub movies_directory: String,        // Movies output directory
    pub tv_directory: String,            // TV output directory
    pub rip_output_directory: Option<String>, // Disc rip output directory
    pub verbose: bool,                   // Enable debug logging
    pub tmdb_api_key: Option<String>,    // TMDb API key
    pub omdb_api_key: Option<String>,    // OMDb API key
    pub makemkvcon_path: Option<String>, // Deprecated: MakeMKV binary path
    pub ripping: RippingConfig,          // Ripping backend config
    pub healthchecks: HealthchecksConfig, // Healthchecks.io config
    pub host: String,                    // Web server bind address
    pub port: u16,                       // Web server bind port
}
```

### `RippingConfig`

```rust
pub struct RippingConfig {
    pub command: Option<String>,         // MakeMKV command (preferred)
    pub makemkvcon_path: Option<String>, // Deprecated: binary path
    pub autorip_eject_after: bool,       // Eject after auto-rip (default: true)
}
```

### `HealthchecksConfig`

```rust
pub struct HealthchecksConfig {
    pub uuid: String,                    // Healthchecks.io UUID
    pub ping_interval_minutes: u64,      // Ping interval (default: 5)
}
```

---

## Default Values

| Field | Default |
|---|---|
| `database_path` | `"riply.db"` |
| `scan_directory` | `None` |
| `movies_directory` | `"movies"` |
| `tv_directory` | `"tv"` |
| `rip_output_directory` | `None` |
| `verbose` | `false` |
| `tmdb_api_key` | `None` |
| `omdb_api_key` | `None` |
| `makemkvcon_path` | `None` |
| `ripping.command` | `None` |
| `ripping.autorip_eject_after` | `true` |
| `healthchecks.uuid` | `""` (empty = disabled) |
| `healthchecks.ping_interval_minutes` | `5` |
| `host` | `"0.0.0.0"` |
| `port` | `3000` |

---

## API

### `Config::load(path: &str) -> Result<Self, anyhow::Error>`

Load configuration from a TOML file.

**Behavior:**
1. Reads file content from `path`
2. Parses as TOML using `toml::from_str`
3. Returns `Config` on success

**Error handling:**
- File not found: `anyhow::Error` (OS error)
- Invalid TOML: `anyhow::Error` (parse error)

### `Config::save(&self, path: &str) -> Result<(), anyhow::Error>`

Save configuration to a TOML file.

**Behavior:**
1. Serializes to pretty-printed TOML using `toml::to_string_pretty`
2. Writes to `path`

### `Config::create_default(path: &str) -> Result<(), anyhow::Error>`

Create a default configuration file.

**Behavior:**
1. Creates `Config::default()`
2. Saves to `path`

### `Config::ripping_command(&self) -> Option<&str>`

Resolve the ripping command with priority: `ripping.command` > `makemkvcon_path` (deprecated).

### `Config::ripping.resolved_command(&self) -> Option<&str>`

Resolve within `RippingConfig`: `command` > `makemkvcon_path` (deprecated).

---

## Path Helpers

| Method | Returns |
|---|---|
| `database_path_buf()` | `PathBuf::from(&self.database_path)` |
| `movies_directory_buf()` | `PathBuf::from(&self.movies_directory)` |
| `tv_directory_buf()` | `PathBuf::from(&self.tv_directory)` |

---

## Validation

### `Config::validate() -> Result<(), String>`

Validate the full configuration.

**Checks:**
- `database_path` is not empty
- `movies_directory` is not empty
- `tv_directory` is not empty
- `port` is between 1 and 65535

### `Config::validate_for_ripping() -> Result<(), String>`

Validate configuration for ripping.

**Checks:**
- `rip_output_directory` is set and not empty
- Parent directory of `rip_output_directory` exists

---

## Deprecation

### `makemkvcon_path` (top-level)

**Status:** Deprecated. Use `ripping.command` instead.

**Migration:**
- Old: `makemkvcon_path = "/usr/bin/makemkvcon"`
- New: `ripping.command = "makemkvcon"`

**Resolution priority:**
1. `ripping.command` (if set and non-empty)
2. `ripping.makemkvcon_path` (if set)
3. Top-level `makemkvcon_path` (if set, fallback)

---

## Where This Lives

| Concern | File |
|---|---|
| Config struct | `infra/src/config.rs` |
| RippingConfig | `infra/src/config.rs` |
| HealthchecksConfig | `infra/src/config.rs` |
| Load/Save | `infra/src/config.rs` |
| Validation | `infra/src/config.rs` |
| CLI config loading | `cli/src/main.rs` |
| Web serve config | `cli/src/commands/serve.rs` |

---

## Notes

- **TOML format** — Uses `serde` derive macros for serialization
- **Optional fields** — Unset fields get default values
- **API keys are optional** — If not set, metadata search falls back to filename-only
- **Config is session-scoped** — Not persisted to database
- **Config can be overridden** — CLI flags (`--host`, `--port`) override config values
- **Healthchecks is opt-in** — Set `uuid` to enable heartbeat pings
- **Validation is manual** — `validate()` and `validate_for_ripping()` must be called explicitly
