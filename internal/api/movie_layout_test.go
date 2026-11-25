package api

import (
	"context"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"riply/internal/library"
	"riply/internal/naming"
	"riply/internal/rip"
)

// movieRipper writes five MKVs with strongly varying sizes — one 40 MB main
// feature plus four small extras, the exact shape classifyRip's size heuristic
// misclassifies as TV (min/max ratio far below 0.5). The TMDb match is a
// movie, so the resolved media type must force the movie layout regardless.
type movieRipper struct{}

func (r *movieRipper) GetDiscInfo(_ context.Context, _ string) (rip.DiscInfo, error) {
	return rip.DiscInfo{Title: "MASK"}, nil
}

func (r *movieRipper) Rip(ctx context.Context, _ string, outDir string, _ int64) <-chan rip.RipProgress {
	ch := make(chan rip.RipProgress, 8)
	go func() {
		defer close(ch)
		_ = os.MkdirAll(outDir, 0o755)
		// Main feature: largest file by far.
		_ = os.WriteFile(filepath.Join(outDir, "title_t01.mkv"), make([]byte, 40<<20), 0o644)
		// Four small extras.
		for i, name := range []string{"title_t00.mkv", "title_t02.mkv", "title_t03.mkv", "title_t04.mkv"} {
			_ = os.WriteFile(filepath.Join(outDir, name), make([]byte, (i+1)<<20), 0o644)
		}
		select {
		case <-ctx.Done():
		case ch <- rip.RipProgress{Status: "complete", Percent: 100}:
		}
	}()
	return ch
}

func (r *movieRipper) Eject(_ context.Context, _ string) error { return nil }
func (r *movieRipper) Version() string                         { return "test" }
func (r *movieRipper) IsAvailable() bool                       { return true }

// Regression: a movie TMDb match on a disc whose file sizes classify as TV
// must still land in the movie layout — main at {Title} ({Year})/{Title} ({Year}).mkv,
// extras under Extras/ — never Season N/S01E0x. The queued drive-accept path
// is what mis-filed the real MASK rip.
func TestMovieTMDBMatchLandsMovieLayout(t *testing.T) {
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	s, _ := newTVServer(t, &movieRipper{}, st)

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/accept", map[string]any{
		"tmdb_id": 12345,
		"title":   "The Mask",
		"year":    1994,
		"is_tv":   false,
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
	titleDir := filepath.Join(moviesDir, "The Mask (1994)")
	mainFile := filepath.Join(titleDir, "The Mask (1994).mkv")
	if _, err := os.Stat(mainFile); err != nil {
		t.Fatalf("missing main movie file: %v", err)
	}

	// Extras must land under Extras/ with their original names.
	extrasDir := filepath.Join(titleDir, "Extras")
	for _, name := range []string{"title_t00.mkv", "title_t02.mkv", "title_t03.mkv", "title_t04.mkv"} {
		if _, err := os.Stat(filepath.Join(extrasDir, name)); err != nil {
			t.Errorf("missing extra %s: %v", name, err)
		}
	}

	// No TV layout may exist anywhere under the title folder.
	if _, err := os.Stat(filepath.Join(titleDir, "Season 1")); !os.IsNotExist(err) {
		t.Error("Season 1 directory produced for a movie TMDb match")
	}
	var tvFiles []string
	_ = filepath.Walk(moviesDir, func(path string, info os.FileInfo, err error) error {
		if err == nil && !info.IsDir() && containsTVMarker(info.Name()) {
			tvFiles = append(tvFiles, path)
		}
		return nil
	})
	if len(tvFiles) > 0 {
		t.Errorf("TV-named files produced for a movie match: %v", tvFiles)
	}

	// The session must advertise the resolved movie type so the UI badge is right.
	s.mu.Lock()
	sess := s.rips[id]
	mt, season := sess.MediaType, sess.SeasonNumber
	s.mu.Unlock()
	if mt != "movie" {
		t.Errorf("session media_type = %q, want movie", mt)
	}
	if season != 0 {
		t.Errorf("session season_number = %d, want 0 for a movie", season)
	}

	// The library entry must record the movie layout. Run a scan so the
	// moved files are indexed (ParsePath derives media_type from the path).
	if rec := doJSON(t, s, http.MethodPost, "/api/library/scan", nil); rec.Code != http.StatusOK && rec.Code != http.StatusAccepted {
		t.Fatalf("scan = %d %s", rec.Code, rec.Body.String())
	}
	var snap struct {
		Entries []struct {
			OutputPath string `json:"output_path"`
			Title      string `json:"title"`
			MediaType  string `json:"media_type"`
			Season     int    `json:"season"`
		} `json:"entries"`
	}
	deadline := time.Now().Add(10 * time.Second)
	for {
		rec := doJSON(t, s, http.MethodGet, "/api/library", nil)
		if err := json.Unmarshal(rec.Body.Bytes(), &snap); err != nil {
			t.Fatal(err)
		}
		if len(snap.Entries) > 0 || time.Now().After(deadline) {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	found := false
	for _, e := range snap.Entries {
		if e.Title != "The Mask" {
			continue
		}
		found = true
		if e.MediaType != "movie" {
			t.Errorf("entry %q media_type = %q, want movie", e.OutputPath, e.MediaType)
		}
		if e.Season != 0 {
			t.Errorf("entry %q season = %d, want 0", e.OutputPath, e.Season)
		}
	}
	if !found {
		t.Error("The Mask missing from library entries")
	}
}

// containsTVMarker reports whether a file name carries a TV episode marker.
func containsTVMarker(name string) bool {
	return strings.Contains(name, "S01E")
}

// TestDriveAcceptEmptyBodyPreservesYear verifies that a front-end drive-level
// Accept (POST with empty body) copies the full stored TMDb match, including
// the year, so the move lands in {Title} ({Year})/{Title} ({Year}).mkv.
func TestDriveAcceptEmptyBodyPreservesYear(t *testing.T) {
	s, _ := newRipServer(t, &movieRipper{})

	// Seed a TMDb match but send an empty-body accept, mirroring the UI.
	s.mu.Lock()
	s.namingMatches["/dev/sr0"] = naming.Result{ID: 12345, Title: "The Mask", Year: 1994, IsTV: false}
	s.mu.Unlock()

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/accept", map[string]any{})
	if rec.Code != http.StatusOK {
		t.Fatalf("drive accept = %d %s", rec.Code, rec.Body.String())
	}

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "accepted" }, 10*time.Second); err != nil {
		t.Fatalf("session never accepted: %v", err)
	}
	waitDone(s, id, 10*time.Second)

	titleDir := filepath.Join(s.cfg.MoviesDir, "The Mask (1994)")
	mainFile := filepath.Join(titleDir, "The Mask (1994).mkv")
	if _, err := os.Stat(mainFile); err != nil {
		t.Fatalf("missing main movie file with year: %v", err)
	}
}
