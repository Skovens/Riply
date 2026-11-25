package api

import (
	"context"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"testing"
	"time"

	"riply/internal/config"
	"riply/internal/drive"
	"riply/internal/library"
	"riply/internal/metrics"
	"riply/internal/rip"
	"riply/internal/sse"
)

// tvRipper writes two MKVs of different sizes and completes immediately.
// Sizes are deliberately close-to-equal so classifyRip could go either way;
// the test forces TV-ness through the accept request metadata.
type tvRipper struct{}

func (r *tvRipper) GetDiscInfo(_ context.Context, _ string) (rip.DiscInfo, error) {
	return rip.DiscInfo{Title: "KLOVN"}, nil
}

func (r *tvRipper) Rip(ctx context.Context, _ string, outDir string, _ int64) <-chan rip.RipProgress {
	ch := make(chan rip.RipProgress, 4)
	go func() {
		defer close(ch)
		_ = os.MkdirAll(outDir, 0o755)
		// title_t01 is the biggest file → becomes episode 1.
		_ = os.WriteFile(filepath.Join(outDir, "title_t01.mkv"), make([]byte, 1<<20), 0o644)
		_ = os.WriteFile(filepath.Join(outDir, "title_t00.mkv"), make([]byte, 512), 0o644)
		select {
		case <-ctx.Done():
		case ch <- rip.RipProgress{Status: "complete", Percent: 100}:
		}
	}()
	return ch
}

func (r *tvRipper) Eject(_ context.Context, _ string) error { return nil }
func (r *tvRipper) Version() string                       { return "test" }
func (r *tvRipper) IsAvailable() bool                     { return true }

func newTVServer(t *testing.T, ripper rip.Ripper, lib *library.Store) (*Server, *drive.Scanner) {
	t.Helper()
	scanner := drive.New()
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DriveDiscovered, Path: "/dev/sr0"})
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DiscInserted, Path: "/dev/sr0"})
	cfg := config.Default()
	cfg.TempDir = t.TempDir()
	cfg.MoviesDir = t.TempDir()
	s := New(cfg, filepath.Join(t.TempDir(), "config.toml"), scanner, ripper, nil, sse.New(), &metrics.Metrics{}, lib)
	return s, scanner
}

// N1 acceptance: drive-level Accept with known TV metadata must land files
// at {Series} ({Year})/Season {N}/{Series} - S{NN}E{NN} - {Title}.mkv, keep
// them TV after a library scan, and advance next-up.
func TestDriveAcceptTVLandsSeasonLayout(t *testing.T) {
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	s, _ := newTVServer(t, &tvRipper{}, st)

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/accept", map[string]any{
		"tmdb_id":    42,
		"title":      "KLOVN",
		"year":       2010,
		"is_tv":      true,
		"tv_season":  1,
		"tv_episode": 1,
	})
	if rec.Code != http.StatusOK {
		t.Fatalf("drive accept = %d %s", rec.Code, rec.Body.String())
	}

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "accepted" }, 10*time.Second); err != nil {
		s.mu.Lock()
		errMsg := ""
		if sess, ok := s.rips[id]; ok {
			errMsg = sess.Error
		}
		s.mu.Unlock()
		t.Fatalf("session never accepted: %v (err=%q)", err, errMsg)
	}
	waitDone(s, id, 10*time.Second)

	moviesDir := s.cfg.MoviesDir
	seasonDir := filepath.Join(moviesDir, "KLOVN (2010)", "Season 1")

	ep1 := filepath.Join(seasonDir, "KLOVN - S01E01 - title_t01.mkv") // biggest → first
	ep2 := filepath.Join(seasonDir, "KLOVN - S01E02 - title_t00.mkv")
	for _, p := range []string{ep1, ep2} {
		if _, err := os.Stat(p); err != nil {
			t.Errorf("missing episode file %v: %v", p, err)
		}
	}
	if _, err := os.Stat(filepath.Join(moviesDir, "KLOVN (2010)", "KLOVN.mkv")); err == nil {
		t.Error("movie-layout file produced for a TV drive accept")
	}

	// Library scan runs asynchronously (202); poll until its entries land.
	if rec := doJSON(t, s, http.MethodPost, "/api/library/scan", nil); rec.Code != http.StatusOK && rec.Code != http.StatusAccepted {
		t.Fatalf("scan = %d %s", rec.Code, rec.Body.String())
	}
	var snap struct {
		Entries []struct {
			OutputPath string `json:"output_path"`
			Title      string `json:"title"`
			MediaType  string `json:"media_type"`
			Season     int    `json:"season"`
			Episode    int    `json:"episode"`
		} `json:"entries"`
		Series []struct {
			Title       string `json:"title"`
			NextSeason  int    `json:"next_season"`
			NextEpisode int    `json:"next_episode"`
		} `json:"series"`
	}
	deadline := time.Now().Add(10 * time.Second)
	for {
		rr := doJSON(t, s, http.MethodGet, "/api/library", nil)
		if err := json.Unmarshal(rr.Body.Bytes(), &snap); err != nil {
			t.Fatal(err)
		}
		if len(snap.Entries) >= 2 || time.Now().After(deadline) {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	tvCount := 0
	for _, e := range snap.Entries {
		if e.Title != "KLOVN" {
			continue
		}
		if e.MediaType != "tv" {
			t.Errorf("entry %q scanned as %q, want tv", e.OutputPath, e.MediaType)
		}
		tvCount++
	}
	if tvCount != 2 {
		t.Errorf("scanned KLOVN tv entries = %d, want 2", tvCount)
	}
	found := false
	for _, se := range snap.Series {
		if se.Title == "KLOVN" {
			found = true
			if se.NextSeason != 1 || se.NextEpisode != 3 {
				t.Errorf("next up = S%02dE%02d, want S01E03", se.NextSeason, se.NextEpisode)
			}
		}
	}
	if !found {
		t.Error("KLOVN missing from series aggregation")
	}
}

// Bounds validation on the drive-accept endpoint (same rule as accept-title).
func TestDriveAcceptRejectsBadTVBounds(t *testing.T) {
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	s, _ := newTVServer(t, &tvRipper{}, st)

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/accept", map[string]any{
		"title": "KLOVN", "is_tv": true, "tv_season": 99999, "tv_episode": 1,
	})
	if rec.Code != http.StatusBadRequest {
		t.Errorf("out-of-bounds drive accept = %d, want 400", rec.Code)
	}
	rec = doJSON(t, s, http.MethodPost, "/api/drives/0/accept", map[string]any{
		"title": "KLOVN", "is_tv": true, "tv_season": -1,
	})
	if rec.Code != http.StatusBadRequest {
		t.Errorf("negative season drive accept = %d, want 400", rec.Code)
	}
}
