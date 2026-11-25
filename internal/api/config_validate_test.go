package api

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// L13: PUT /api/config must reject values that cannot work before mutating
// the live config — bad ports persisted and only failed at the next restart,
// unwritable dirs surfaced mid-rip.
func TestPutConfigRejectsBadValues(t *testing.T) {
	s, _, _, _ := newSeqServer(t)
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	origPort := s.cfg.HTTPPort

	cases := []struct {
		name string
		body string
	}{
		{"port zero", `{"http_port":0}`},
		{"port negative", `{"http_port":-1}`},
		{"port over 65535", `{"http_port":70000}`},
		{"port string", `{"http_port":"8080"}`},
		{"movies_dir empty", `{"movies_dir":""}`},
		{"temp_dir empty", `{"temp_dir":""}`},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			rec := httptest.NewRecorder()
			mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(c.body)))
			if rec.Code != http.StatusBadRequest {
				t.Fatalf("%s: PUT = %d, want 400 (body: %s)", c.name, rec.Code, rec.Body.String())
			}
			if s.cfg.HTTPPort != origPort {
				t.Errorf("%s: http_port mutated to %d, want %d", c.name, s.cfg.HTTPPort, origPort)
			}
		})
	}
}

func TestPutConfigRejectsUnwritableDirs(t *testing.T) {
	// Root bypasses directory permission bits, so the "unwritable" premise
	// does not hold when tests run as root (this container); CI runs unprivileged.
	if os.Geteuid() == 0 {
		t.Skip("running as root: read-only directories are still writable")
	}
	s, _, _, _ := newSeqServer(t)
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	origMovies, origTemp := s.cfg.MoviesDir, s.cfg.TempDir

	// A read-only directory (non-root tests honor the mode).
	ro := filepath.Join(t.TempDir(), "readonly")
	if err := os.MkdirAll(ro, 0o500); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(ro, 0o700) })

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(`{"temp_dir":"`+ro+`"}`)))
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("PUT unwritable temp_dir = %d, want 400 (body: %s)", rec.Code, rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "temp_dir") {
		t.Errorf("400 body should name the offending key, got %q", rec.Body.String())
	}
	if s.cfg.TempDir != origTemp {
		t.Errorf("temp_dir mutated to %q despite rejection", s.cfg.TempDir)
	}

	// All-or-nothing: valid movies_dir + invalid temp_dir rejects BOTH.
	rec = httptest.NewRecorder()
	body := `{"movies_dir":"` + t.TempDir() + `","temp_dir":"` + ro + `"}`
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(body)))
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("mixed PUT = %d, want 400", rec.Code)
	}
	if s.cfg.MoviesDir != origMovies {
		t.Errorf("movies_dir mutated despite invalid temp_dir in same request")
	}
}

func TestPutConfigAcceptsValidValues(t *testing.T) {
	s, _, _, _ := newSeqServer(t)
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)

	// Boundary ports and a directory that does not exist yet (lazy-creation
	// is allowed, matching the rip-start path).
	absent := filepath.Join(t.TempDir(), "sub", "dir")
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(`{"http_port":65535,"temp_dir":"`+absent+`"}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("PUT valid = %d, want 200 (body: %s)", rec.Code, rec.Body.String())
	}
	var cfg map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &cfg); err != nil {
		t.Fatal(err)
	}
	if cfg["http_port"] != float64(65535) {
		t.Errorf("http_port = %v, want 65535", cfg["http_port"])
	}
	if cfg["temp_dir"] != absent {
		t.Errorf("temp_dir = %v, want %q", cfg["temp_dir"], absent)
	}
	rec = httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(`{"http_port":1}`)))
	if rec.Code != http.StatusOK {
		t.Errorf("port 1 = %d, want 200", rec.Code)
	}
}

// A "~/Movies" PUT from Settings must be expanded before validation and
// assignment, otherwise the server creates a literal "~" directory.
func TestPutConfigExpandsTildeDirs(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	s, _, _, _ := newSeqServer(t)
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(`{"movies_dir":"~/Movies"}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("PUT ~/Movies = %d, want 200 (body: %s)", rec.Code, rec.Body.String())
	}
	want := filepath.Join(home, "Movies")
	if s.cfg.MoviesDir != want {
		t.Errorf("MoviesDir = %q, want %q", s.cfg.MoviesDir, want)
	}
	if _, err := os.Stat(filepath.Join(home, "Movies")); err != nil {
		t.Errorf("expanded directory not created: %v", err)
	}
}
