package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"

	"riply/internal/logger"
)

// ProviderConfig holds one metadata provider's settings.
type ProviderConfig struct {
	APIKey string `toml:"api_key" json:"api_key"`
}

// MetadataConfig is the per-media-type provider try-order ([metadata] in
// config.toml): the first entry is the primary provider; the chain falls
// back to later entries on error or empty results.
type MetadataConfig struct {
	Movies []string `toml:"movies" json:"movies,omitempty"`
	TV     []string `toml:"tv" json:"tv,omitempty"`
}

// Config holds the application configuration.
type Config struct {
	MoviesDir string `toml:"movies_dir" json:"movies_dir"`
	// TVShowsDir is the destination root for TV shows ("Show (Year)/Season N/…").
	// Empty means "follow movies_dir" — the pre-tv_shows_dir behavior — so
	// existing installs keep landing TV next to movies until they opt in.
	TVShowsDir string `toml:"tv_shows_dir" json:"tv_shows_dir"`
	TempDir    string `toml:"temp_dir" json:"temp_dir"`
	// Providers holds per-provider settings ([providers.tmdb] etc.); the
	// legacy flat tmdb_api_key is migrated into [providers.tmdb] on Load.
	Providers map[string]ProviderConfig `toml:"providers" json:"providers,omitempty"`
	Metadata  MetadataConfig            `toml:"metadata" json:"metadata,omitempty"`
	HTTPHost  string                    `toml:"http_host" json:"http_host"`
	HTTPPort  int                       `toml:"http_port" json:"http_port"`
	Makemkv   string                    `toml:"makemkv_path" json:"makemkv_path"`
	AutoSequence bool `toml:"auto_sequence" json:"auto_sequence"` // pre-fill the search modal from last rip; default true
	AutoRip      bool `toml:"autorip" json:"autorip"`             // automatically eject after accept and start ripping new discs; default true
	MinTitleLength int  `toml:"min_title_length" json:"min_title_length"` // seconds; makemkvcon --minlength, 0 = omit the flag (rip every title)
	// TempFillLimitPercent blocks a rip when temp_dir usage (current + estimated
	// disc size) would exceed this percentage. 0 disables the check.
	TempFillLimitPercent int `toml:"temp_fill_limit_percent" json:"temp_fill_limit_percent"`
	// MoveFillLimitPercent blocks the move to movies_dir when its filesystem
	// usage (current + rip output size) would exceed this percentage. 0 disables.
	MoveFillLimitPercent int `toml:"move_fill_limit_percent" json:"move_fill_limit_percent"`
}

// Default returns a Config with sensible defaults.
func Default() *Config {
	home, _ := os.UserHomeDir()
	return &Config{
		MoviesDir:            filepath.Join(home, "Movies"),
		TempDir:              "/tmp/riply",
		Providers:            map[string]ProviderConfig{"tmdb": {}, "omdb": {}},
		Metadata:             MetadataConfig{Movies: []string{"tmdb"}, TV: []string{"tmdb"}},
		HTTPHost:             "127.0.0.1",
		HTTPPort:             3000,
		Makemkv:              "",
		AutoSequence:         true,
		AutoRip:              true,
		MinTitleLength:       300,
		TempFillLimitPercent: 80,
		MoveFillLimitPercent: 95,
	}
}

// ProviderKey returns one provider's configured API key ("" when unset or
// the provider is unknown).
func (c *Config) ProviderKey(name string) string {
	if c.Providers == nil {
		return ""
	}
	return c.Providers[name].APIKey
}

// legacyTMDBKey is the pre-[providers] flat config key. Decode a config file
// into this alongside Config to migrate the value: toml.MetaData exposes key
// paths only, not values, so the raw value needs its own decode target.
type legacyTMDBKey struct {
	TMDBKey string `toml:"tmdb_api_key"`
}

// TVDir returns the destination root for TV shows: TVShowsDir when set,
// otherwise MoviesDir (the pre-tv_shows_dir behavior).
func (c *Config) TVDir() string {
	if c.TVShowsDir != "" {
		return c.TVShowsDir
	}
	return c.MoviesDir
}

// DefaultConfigPath returns the XDG-compliant config path.
func DefaultConfigPath() string {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return filepath.Join(xdg, "riply", "config.toml")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".config", "riply", "config.toml")
}

// Load reads config from the given path, falling back to defaults for missing fields.
// If path is empty, uses DefaultConfigPath().
// If the file doesn't exist, auto-creates it with defaults.
func Load(path string) (*Config, error) {
	if path == "" {
		path = DefaultConfigPath()
	}

	cfg := Default()

	if _, err := os.Stat(path); os.IsNotExist(err) {
		// Auto-create on first run
		if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
			return nil, fmt.Errorf("cannot create config directory: %w", err)
		}
		if err := cfg.Save(path); err != nil {
			return nil, fmt.Errorf("cannot create default config: %w", err)
		}
		logger.Info("Config created", "path", path)
		return cfg, nil
	}

	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	if _, err := toml.NewDecoder(f).Decode(cfg); err != nil {
		logger.Warn("Config parse failed", "error", err)
		return nil, err
	}
	// Legacy flat tmdb_api_key → [providers.tmdb] api_key. A second decode
	// into a minimal shadow struct reads the raw value (toml.MetaData only
	// gives key paths); the nested value wins when both are present.
	if cfg.Providers == nil {
		cfg.Providers = map[string]ProviderConfig{"tmdb": {}, "omdb": {}}
	}
	if f2, err := os.Open(path); err == nil {
		var legacy legacyTMDBKey
		_, _ = toml.NewDecoder(f2).Decode(&legacy)
		f2.Close()
		if legacy.TMDBKey != "" {
			pc := cfg.Providers["tmdb"]
			if pc.APIKey == "" {
				pc.APIKey = legacy.TMDBKey
				cfg.Providers["tmdb"] = pc
				logger.Info("Migrated legacy tmdb_api_key into [providers.tmdb]")
			}
		}
	}
	// Missing/empty [metadata] keeps the pre-adapter behavior: TMDb only.
	if len(cfg.Metadata.Movies) == 0 {
		cfg.Metadata.Movies = []string{"tmdb"}
	}
	if len(cfg.Metadata.TV) == 0 {
		cfg.Metadata.TV = []string{"tmdb"}
	}
	// "~" paths in config files must work like they do in a shell: an
	// unexpanded "~/Movies" would otherwise create a literal "~" directory
	// under the server's working directory.
	cfg.MoviesDir = ExpandHome(cfg.MoviesDir)
	cfg.TVShowsDir = ExpandHome(cfg.TVShowsDir)
	cfg.TempDir = ExpandHome(cfg.TempDir)

	logger.Info("Config loaded", "path", path)

	return cfg, nil
}

// ExpandHome resolves a leading "~" or "~/" in p to the current user's home
// directory (shell semantics). All other values are returned unchanged,
// including "~user" forms, which have no portable resolution.
func ExpandHome(p string) string {
	if p != "~" && !strings.HasPrefix(p, "~/") {
		return p
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return p
	}
	if p == "~" {
		return home
	}
	return filepath.Join(home, p[2:])
}

// Save writes the config to the given path atomically: encode into a temp
// file in the same directory, fsync, then rename over the target. A crash or
// power loss mid-write leaves the previous config intact instead of a
// truncated file that would silently fall back to defaults on next boot.
func (c *Config) Save(path string) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("cannot create config directory: %w", err)
	}

	tmp, err := os.CreateTemp(dir, ".config-*.tmp")
	if err != nil {
		return fmt.Errorf("cannot create temp config file: %w", err)
	}
	tmpName := tmp.Name()

	if err := toml.NewEncoder(tmp).Encode(c); err != nil {
		tmp.Close()
		os.Remove(tmpName)
		return fmt.Errorf("cannot encode config: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		os.Remove(tmpName)
		return fmt.Errorf("cannot sync config: %w", err)
	}
	if err := tmp.Close(); err != nil {
		os.Remove(tmpName)
		return fmt.Errorf("cannot close config: %w", err)
	}
	if err := os.Rename(tmpName, path); err != nil {
		os.Remove(tmpName)
		return fmt.Errorf("cannot replace config file: %w", err)
	}
	return nil
}
