package api

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"riply/internal/config"
	"riply/internal/drive"
	"riply/internal/library"
	"riply/internal/metrics"
	"riply/internal/naming"
	"riply/internal/sse"
)

func newLibServer(t *testing.T) (*Server, *library.Store) {
	t.Helper()
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	cfg := config.Default()
	cfg.MoviesDir = t.TempDir()
	cfg.TempDir = filepath.Join(t.TempDir(), "riply")
	s := New(cfg, filepath.Join(t.TempDir(), "config.toml"), nil, nil, nil, sse.New(), &metrics.Metrics{}, st)
	return s, st
}

func TestAFillTV(t *testing.T) {
	s, st := newLibServer(t)
	ctx := context.Background()

	// Explicit disc marker wins over the library.
	if err := st.Add(ctx, library.Entry{
		Title: "KLOVN", MediaType: "tv", Season: 2, Episode: 1,
		OutputPath: "/lib/KLOVN (2010)/Season 2/KLOVN - S02E01 - X.mkv",
	}); err != nil {
		t.Fatal(err)
	}
	if season, episode := s.autofillTV("KLOVN - S02E05 - X", &naming.Result{Title: "KLOVN", IsTV: true}, "tv"); season != 2 || episode != 5 {
		t.Errorf("explicit marker = (%d,%d), want (2,5)", season, episode)
	}

	// Season hint only: continue after the stored highest episode.
	if season, episode := s.autofillTV("KLOVN S02", &naming.Result{Title: "KLOVN", IsTV: true}, "tv"); season != 2 || episode != 2 {
		t.Errorf("season hint = (%d,%d), want (2,2)", season, episode)
	}

	// Movies never autofill.
	if season, episode := s.autofillTV("Inception", nil, "movie"); season != 0 || episode != 0 {
		t.Errorf("movie = (%d,%d), want (0,0)", season, episode)
	}

	// No library hit: defaults to S1E1.
	if season, episode := s.autofillTV("Brand New Show", &naming.Result{Title: "Brand New Show", IsTV: true}, "tv"); season != 1 || episode != 1 {
		t.Errorf("unknown show = (%d,%d), want (1,1)", season, episode)
	}
}

func newServerWithLib(t *testing.T, lib *library.Store, moviesDir string) *Server {
	t.Helper()
	cfg := config.Default()
	cfg.MoviesDir = moviesDir
	cfg.TempDir = filepath.Join(t.TempDir(), "riply")
	return New(cfg, filepath.Join(t.TempDir(), "config.toml"), nil, nil, nil, sse.New(), &metrics.Metrics{}, lib)
}

func post(t *testing.T, s *Server, path string) *httptest.ResponseRecorder {
	t.Helper()
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, path, nil))
	return rec
}

func TestLibraryHandlersUnavailable(t *testing.T) {
	// A nil store (database failed to open) must degrade to 503, never panic.
	s := newServerWithLib(t, nil, t.TempDir())
	for _, path := range []string{"/api/library/scan", "/api/library/reset"} {
		if rec := post(t, s, path); rec.Code != http.StatusServiceUnavailable {
			t.Errorf("POST %s with nil store = %d, want 503", path, rec.Code)
		}
	}
	rec := httptest.NewRecorder()
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/library", nil))
	if rec.Code != http.StatusServiceUnavailable {
		t.Errorf("GET /api/library with nil store = %d, want 503", rec.Code)
	}
}

func TestScanLibraryRejectsMissingMoviesDir(t *testing.T) {
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	// Unconfigured movies_dir.
	s := newServerWithLib(t, st, "  ")
	if rec := post(t, s, "/api/library/scan"); rec.Code != http.StatusBadRequest {
		t.Errorf("scan with blank movies_dir = %d, want 400", rec.Code)
	}

	// Configured but nonexistent.
	s = newServerWithLib(t, st, filepath.Join(t.TempDir(), "missing"))
	if rec := post(t, s, "/api/library/scan"); rec.Code != http.StatusBadRequest {
		t.Errorf("scan with missing movies_dir = %d, want 400", rec.Code)
	}
}

func TestLibraryConflictsWithRunningScan(t *testing.T) {
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	s := newServerWithLib(t, st, t.TempDir())

	// Hold the scan lock to simulate an in-flight scan deterministically.
	s.scanMu.Lock()
	if rec := post(t, s, "/api/library/scan"); rec.Code != http.StatusConflict {
		t.Errorf("second scan while one runs = %d, want 409", rec.Code)
	}
	if rec := post(t, s, "/api/library/reset"); rec.Code != http.StatusConflict {
		t.Errorf("reset during a scan = %d, want 409", rec.Code)
	}
	s.scanMu.Unlock()

	// With the lock free, reset succeeds and clears the index.
	if err := st.Add(context.Background(), library.Entry{
		Title: "Heat", MediaType: "movie", OutputPath: "/lib/Heat.mkv",
	}); err != nil {
		t.Fatal(err)
	}
	if rec := post(t, s, "/api/library/reset"); rec.Code != http.StatusOK {
		t.Fatalf("reset with free lock = %d, want 200 (body %s)", rec.Code, rec.Body.String())
	}
	snap, err := st.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Entries) != 0 {
		t.Errorf("entries after reset = %d, want 0", len(snap.Entries))
	}
}

func TestRecordRipEntryAbsolutePath(t *testing.T) {
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	s := newServerWithLib(t, st, t.TempDir())

	s.recordRipEntry(library.Entry{
		Title:      "Relative Rip",
		MediaType:  "movie",
		OutputPath: filepath.Join("movies", "Relative Rip (2001)", "Relative Rip (2001).mkv"),
	})

	snap, err := st.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Entries) != 1 {
		t.Fatalf("entries = %d, want 1", len(snap.Entries))
	}
	got := snap.Entries[0].OutputPath
	if !filepath.IsAbs(got) {
		t.Errorf("recorded path = %q, want an absolute path so a scan can flag it missing", got)
	}
	if snap.Entries[0].Source != "rip" {
		t.Errorf("source = %q, want rip", snap.Entries[0].Source)
	}
}

func TestAcceptRecordsHistory(t *testing.T) {
	s, st := newLibServer(t)

	ripDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(ripDir, "main.mkv"), make([]byte, 4096), 0644); err != nil {
		t.Fatal(err)
	}

	s.mu.Lock()
	s.ripCount = 1
	s.rips[1] = &RipSession{
		ID:        1,
		State:     "complete",
		OutputDir: ripDir,
		Match:     &naming.Result{ID: 27205, ExtID: "27205", Provider: "tmdb", Title: "Inception", Year: 2010},
		MediaType: "movie",
		DiscName:  "INCEPTION",
	}
	s.mu.Unlock()

	body := strings.NewReader(`{}`)
	req := httptest.NewRequest(http.MethodPost, "/api/rips/1/accept", body)
	rec := httptest.NewRecorder()
	handler := http.NewServeMux()
	s.RegisterRoutes(handler)
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body: %s)", rec.Code, rec.Body.String())
	}
	var moved struct {
		MainFile string `json:"main_file"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &moved); err != nil {
		t.Fatal(err)
	}
	mainFile := moved.MainFile
	if mainFile == "" {
		t.Fatalf("move result missing main_file: %s", rec.Body.String())
	}

	snap, err := st.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Entries) != 1 {
		t.Fatalf("library entries = %d, want 1", len(snap.Entries))
	}
	e := snap.Entries[0]
	if e.Title != "Inception" || e.Year != 2010 || e.MediaType != "movie" || e.Source != "rip" {
		t.Errorf("recorded entry = %+v", e)
	}
	if e.OutputPath != mainFile {
		t.Errorf("recorded path = %q, want moved file %q", e.OutputPath, mainFile)
	}
}

// newSeqServer wires a scanner with one ready drive so /api/drives payloads
// (last_rip, auto-sequence toggle) are observable over HTTP.
func newSeqServer(t *testing.T) (*Server, *library.Store, *drive.Scanner, *sse.Manager) {
	t.Helper()
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	cfg := config.Default()
	cfg.MoviesDir = t.TempDir()
	cfg.TempDir = filepath.Join(t.TempDir(), "riply")
	scanner := drive.New()
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DriveDiscovered, Path: "/dev/sr9"})
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DiscInserted, Path: "/dev/sr9"})
	scanner.UpdateState("/dev/sr9", drive.StateReady, "KLOVN")
	sseMgr := sse.New()
	s := New(cfg, filepath.Join(t.TempDir(), "config.toml"), scanner, nil, nil, sseMgr, &metrics.Metrics{}, st)
	return s, st, scanner, sseMgr
}

func addKlovEntry(t *testing.T, st *library.Store) {
	t.Helper()
	if err := st.Add(context.Background(), library.Entry{
		Title: "KLOVN", MediaType: "tv", Season: 1, Episode: 1,
		DiscName:   "KLOVN S01",
		OutputPath: "/x/KLOVN S01/E1.mkv", Source: "rip",
	}); err != nil {
		t.Fatal(err)
	}
}

func TestDuplicateCheckPublishes(t *testing.T) {
	s, st, _, sseMgr := newSeqServer(t)
	addKlovEntry(t, st)

	sub := sseMgr.Subscribe()
	t.Cleanup(func() { sseMgr.Unsubscribe(sub) })

	s.DuplicateCheck(context.Background(), "/dev/sr9", "klovn s01")

	select {
	case ev := <-sub.Chan():
		if ev.Type != "rip_duplicate" {
			t.Fatalf("event type = %q, want rip_duplicate", ev.Type)
		}
		if !strings.Contains(ev.Message, "/x/KLOVN") {
			t.Errorf("message = %q, want it to name the previous output path", ev.Message)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("no event published for a matching disc")
	}

	// A disc name with no history must publish nothing.
	s.DuplicateCheck(context.Background(), "/dev/sr9", "MASK 2")
	select {
	case ev := <-sub.Chan():
		t.Fatalf("unexpected event for unmatched disc: %+v", ev)
	case <-time.After(100 * time.Millisecond):
	}
}

func TestDrivesLastRipPayload(t *testing.T) {
	s, st, _, _ := newSeqServer(t)
	addKlovEntry(t, st)

	get := func() map[string]any {
		t.Helper()
		mux := http.NewServeMux()
		s.RegisterRoutes(mux)
		rec := httptest.NewRecorder()
		mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/drives", nil))
		if rec.Code != http.StatusOK {
			t.Fatalf("GET /api/drives = %d, want 200 (body: %s)", rec.Code, rec.Body.String())
		}
		var out []map[string]any
		if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
			t.Fatal(err)
		}
		if len(out) != 1 {
			t.Fatalf("drives = %d, want 1", len(out))
		}
		return out[0]
	}

	// AutoSequence default true: last_rip present with the newest entry.
	d := get()
	lr, ok := d["last_rip"].(map[string]any)
	if !ok || lr["title"] != "KLOVN" {
		t.Fatalf("last_rip = %v, want KLOVN", d["last_rip"])
	}
	if d["auto_sequence_enabled"] != true {
		t.Errorf("auto_sequence_enabled = %v, want true", d["auto_sequence_enabled"])
	}
	// Disc has a TV library history and no SxxEyy marker: suggestion continues.
	sg, ok := d["suggest"].(map[string]any)
	if !ok || sg["season"] != float64(1) || sg["episode"] != float64(2) {
		t.Errorf("suggest = %v, want S1E2", d["suggest"])
	}

	// Global off: last_rip absent.
	s.mu.Lock()
	s.cfg.AutoSequence = false
	s.mu.Unlock()
	if _, ok := get()["last_rip"]; ok {
		t.Fatal("last_rip present with AutoSequence disabled")
	}
	s.mu.Lock()
	s.cfg.AutoSequence = true
	s.mu.Unlock()

	// Per-drive off: last_rip absent for that drive.
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/drives/0/auto-sequence", strings.NewReader(`{"enabled":false}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("toggle = %d, want 200 (body: %s)", rec.Code, rec.Body.String())
	}
	d = get()
	if _, ok := d["last_rip"]; ok {
		t.Fatal("last_rip present with per-drive Auto-Sequence disabled")
	}
	if d["auto_sequence_enabled"] != false {
		t.Errorf("auto_sequence_enabled = %v, want false after toggle", d["auto_sequence_enabled"])
	}
}

func TestConfigAutoSequenceRoundTrip(t *testing.T) {
	s, _, _, _ := newSeqServer(t)
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)

	req := httptest.NewRequest(http.MethodGet, "/api/config", nil)
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	var cfg map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &cfg); err != nil {
		t.Fatal(err)
	}
	if cfg["auto_sequence"] != true {
		t.Fatalf("GET /api/config auto_sequence = %v, want true", cfg["auto_sequence"])
	}

	req = httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(`{"auto_sequence":false}`))
	rec = httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("PUT /api/config = %d, want 200 (body: %s)", rec.Code, rec.Body.String())
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &cfg); err != nil {
		t.Fatal(err)
	}
	if cfg["auto_sequence"] != false {
		t.Fatalf("PUT response auto_sequence = %v, want false", cfg["auto_sequence"])
	}

	// Persisted to the config file on disk.
	b, err := os.ReadFile(s.cfgPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "auto_sequence = false") {
		t.Fatalf("config file:\n%s\nwant auto_sequence = false", string(b))
	}
}

// N2: PUT /api/config min_title_length must update the config (response +
// persisted file) and propagate to the live ripper via MinLengthSetter.
func TestConfigMinTitleLengthRoundTrip(t *testing.T) {
	s, _, _, _ := newSeqServer(t)
	fake := &recordingRipper{}
	s.ripper = fake
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(`{"min_title_length":120}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("PUT = %d, want 200 (body: %s)", rec.Code, rec.Body.String())
	}
	var cfg map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &cfg); err != nil {
		t.Fatal(err)
	}
	if cfg["min_title_length"] != float64(120) {
		t.Fatalf("response min_title_length = %v, want 120", cfg["min_title_length"])
	}
	if fake.gotMinLen() != 120 {
		t.Errorf("live ripper min length = %d, want 120", fake.gotMinLen())
	}
	b, err := os.ReadFile(s.cfgPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "min_title_length = 120") {
		t.Fatalf("config file lacks min_title_length:\n%s", b)
	}

	// 0 disables the flag and propagates too.
	rec = httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/config", strings.NewReader(`{"min_title_length":0}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("PUT 0 = %d, want 200", rec.Code)
	}
	if fake.gotMinLen() != 0 {
		t.Errorf("live ripper min length after 0 = %d, want 0", fake.gotMinLen())
	}
}
