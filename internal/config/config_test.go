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
		TMDBKey:   "secret-key",
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
	if *got != *cfg {
		t.Errorf("round-trip mismatch:\n got %+v\nwant %+v", *got, *cfg)
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
