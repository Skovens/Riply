package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"

	"riply/internal/logger"
)

// Config holds the application configuration.
type Config struct {
	MoviesDir      string `toml:"movies_dir" json:"movies_dir"`
	TempDir        string `toml:"temp_dir" json:"temp_dir"`
	TMDBKey        string `toml:"tmdb_api_key" json:"tmdb_api_key"`
	HTTPHost       string `toml:"http_host" json:"http_host"`
	HTTPPort       int    `toml:"http_port" json:"http_port"`
	Makemkv        string `toml:"makemkv_path" json:"makemkv_path"`
	AutoSequence   bool   `toml:"auto_sequence" json:"auto_sequence"`       // pre-fill the search modal from last rip; default true
	AutoRip        bool   `toml:"autorip" json:"autorip"`                   // automatically eject after accept and start ripping new discs; default true
	MinTitleLength int    `toml:"min_title_length" json:"min_title_length"` // seconds; makemkvcon --minlength, 0 = omit the flag (rip every title)
}

// Default returns a Config with sensible defaults.
func Default() *Config {
	home, _ := os.UserHomeDir()
	return &Config{
		MoviesDir:      filepath.Join(home, "Movies"),
		TempDir:        "/tmp/riply",
		TMDBKey:        "",
		HTTPHost:       "127.0.0.1",
		HTTPPort:       3000,
		Makemkv:        "",
		AutoSequence:   true,
		AutoRip:        true,
		MinTitleLength: 300,
	}
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
	// "~" paths in config files must work like they do in a shell: an
	// unexpanded "~/Movies" would otherwise create a literal "~" directory
	// under the server's working directory.
	cfg.MoviesDir = ExpandHome(cfg.MoviesDir)
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
