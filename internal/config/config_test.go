package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSaveLoadRoundTrip(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sub", "config.toml")
	cfg := &Config{
		MoviesDir: "/data/movies",
		TempDir:   "/data/rip",
		Providers: map[string]ProviderConfig{"tmdb": {APIKey: "secret-key"}, "omdb": {APIKey: "omdb-key"}},
		Metadata:  MetadataConfig{Movies: []string{"omdb", "tmdb"}, TV: []string{"tmdb"}},
		HTTPHost:  "0.0.0.0",
		HTTPPort:  8123,
		Makemkv:   "/usr/bin/makemkvcon",
	}
	if err := cfg.Save(path); err != nil {
		t.Fatalf("Save: %v", err)
	}

	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if got.HTTPHost != cfg.HTTPHost || got.HTTPPort != cfg.HTTPPort ||
		got.MoviesDir != cfg.MoviesDir || got.TempDir != cfg.TempDir ||
		got.Makemkv != cfg.Makemkv ||
		got.ProviderKey("tmdb") != cfg.ProviderKey("tmdb") ||
		got.ProviderKey("omdb") != cfg.ProviderKey("omdb") ||
		strings.Join(got.Metadata.Movies, ",") != strings.Join(cfg.Metadata.Movies, ",") ||
		strings.Join(got.Metadata.TV, ",") != strings.Join(cfg.Metadata.TV, ",") {
		t.Errorf("round-trip mismatch:\n got %+v\nwant %+v", got, cfg)
	}
}

func TestLoadMissingFileCreatesDefaults(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.toml")
	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load on missing file: %v", err)
	}
	def := Default()
	if got.HTTPHost != def.HTTPHost || got.HTTPPort != def.HTTPPort {
		t.Errorf("defaults not applied: %+v", *got)
	}
	// The file must exist now (auto-create).
	if _, err := os.Stat(path); err != nil {
		t.Errorf("config not auto-created: %v", err)
	}
}

func TestLoadMalformedTOMLErrors(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.toml")
	if err := os.WriteFile(path, []byte("this is = not = valid toml [["), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Load(path); err == nil {
		t.Error("malformed TOML must return an error")
	}
}

func TestExpandHome(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Skipf("no home directory: %v", err)
	}
	cases := []struct{ in, want string }{
		{"~/Movies", filepath.Join(home, "Movies")},
		{"~", home},
		{"~/", filepath.Join(home, "")},
		{"~user/Movies", "~user/Movies"},
		{"/abs/Movies", "/abs/Movies"},
		{"relative/Movies", "relative/Movies"},
		{"", ""},
	}
	for _, c := range cases {
		if got := ExpandHome(c.in); got != c.want {
			t.Errorf("ExpandHome(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

// A "~/Movies" in config.toml must resolve like a shell would, otherwise the
// server creates a literal "~" directory under its working directory.
func TestLoadExpandsTildeDirs(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	path := filepath.Join(t.TempDir(), "config.toml")
	if err := os.WriteFile(path, []byte("movies_dir = \"~/Movies\"\ntemp_dir = \"~\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if want := filepath.Join(home, "Movies"); got.MoviesDir != want {
		t.Errorf("MoviesDir = %q, want %q", got.MoviesDir, want)
	}
	if got.TempDir != home {
		t.Errorf("TempDir = %q, want %q", got.TempDir, home)
	}
}

// Save must never leave a truncated or partial file behind: after saving over
// an existing config, the file on disk parses cleanly and contains no temp
// leftovers.
func TestSaveLeavesNoTempFiles(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.toml")
	cfg := Default()
	for i := range 3 {
		cfg.HTTPPort = 3000 + i
		if err := cfg.Save(path); err != nil {
			t.Fatalf("Save %d: %v", i, err)
		}
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if strings.Contains(e.Name(), ".tmp") {
			t.Errorf("temp file left behind: %s", e.Name())
		}
	}
	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load after repeated saves: %v", err)
	}
	if got.HTTPPort != 3002 {
		t.Errorf("final port = %d, want 3002", got.HTTPPort)
	}
}

// A pre-adapter config file (flat tmdb_api_key, no [metadata]) must load
// cleanly: the key migrates into [providers.tmdb] and both try-orders
// default to TMDb.
func TestLoadMigratesLegacyTMDBKey(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.toml")
	body := "movies_dir = \"/m\"\nhttp_port = 3111\ntmdb_api_key = \"legacy-key\"\n"
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if got.ProviderKey("tmdb") != "legacy-key" {
		t.Errorf("ProviderKey(tmdb) = %q, want legacy-key", got.ProviderKey("tmdb"))
	}
	if got.ProviderKey("omdb") != "" {
		t.Errorf("ProviderKey(omdb) = %q, want empty", got.ProviderKey("omdb"))
	}
	if strings.Join(got.Metadata.Movies, ",") != "tmdb" || strings.Join(got.Metadata.TV, ",") != "tmdb" {
		t.Errorf("metadata defaults = %+v, want tmdb for both types", got.Metadata)
	}
}

// A nested config round-trips its [metadata] orders and provider keys.
func TestLoadNestedProvidersAndMetadata(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.toml")
	body := `
[metadata]
movies = ["omdb", "tmdb"]
tv = ["tmdb"]

[providers.tmdb]
api_key = "tmdb-key"

[providers.omdb]
api_key = "omdb-key"
`
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if got.ProviderKey("tmdb") != "tmdb-key" || got.ProviderKey("omdb") != "omdb-key" {
		t.Errorf("provider keys = %q/%q", got.ProviderKey("tmdb"), got.ProviderKey("omdb"))
	}
	if strings.Join(got.Metadata.Movies, ",") != "omdb,tmdb" || strings.Join(got.Metadata.TV, ",") != "tmdb" {
		t.Errorf("metadata orders = %+v", got.Metadata)
	}
}

// Save must never write a provider API key... except intentionally: keys are
// stored in config.toml by design (Settings persists them), so Save writes
// them; this test just pins the nested key names so the file format cannot
// drift silently.
func TestSaveWritesNestedProviderKeys(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.toml")
	cfg := Default()
	cfg.Providers["tmdb"] = ProviderConfig{APIKey: "k1"}
	cfg.Providers["omdb"] = ProviderConfig{APIKey: "k2"}
	if err := cfg.Save(path); err != nil {
		t.Fatalf("Save: %v", err)
	}
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"[providers.tmdb]", "api_key = \"k1\"", "[providers.omdb]", "api_key = \"k2\"", "[metadata]", "movies"} {
		if !strings.Contains(string(body), want) {
			t.Errorf("saved config missing %q:\n%s", want, body)
		}
	}
}
