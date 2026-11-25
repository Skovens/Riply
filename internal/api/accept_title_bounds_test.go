package api

import (
	"net/http"
	"os"
	"path/filepath"
	"testing"
)

// seedOneTitleRip creates a completed rip session with one pending title and
// returns (server, ripID).
func seedOneTitleRip(t *testing.T) (*Server, int) {
	t.Helper()
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.mu.Lock()
	s.ripCount++
	id := s.ripCount
	outDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(outDir, "ep_t00.mkv"), make([]byte, 1<<20), 0o644); err != nil {
		t.Fatal(err)
	}
	sess := &RipSession{
		ID:        id,
		DrivePath: "/dev/sr0",
		DiscName:  "KLOVN S01",
		State:     "complete",
		OutputDir: outDir,
		Done:      make(chan struct{}),
		Titles: []*RipTitle{
			{SourceFile: "ep_t00.mkv", SizeMB: 1, AcceptedName: "KLOVN", State: "pending"},
		},
	}
	close(sess.Done)
	s.rips[id] = sess
	s.mu.Unlock()
	return s, id
}

// M8: negative or absurd tv_season/tv_episode must be rejected with 400
// before reaching the move layer (Season -1/ and S-1E-3 are impossible).
func TestAcceptTitleBoundsCheck(t *testing.T) {
	tests := []struct {
		name       string
		season     int
		episode    int
		wantStatus int
	}{
		{"negative season", -1, 3, http.StatusBadRequest},
		{"negative episode", 1, -3, http.StatusBadRequest},
		{"both negative", -1, -1, http.StatusBadRequest},
		{"season oversized", 10001, 1, http.StatusBadRequest},
		{"episode oversized", 1, 10001, http.StatusBadRequest},
		{"max allowed season", 10000, 0, http.StatusOK},
		{"max allowed episode", 0, 10000, http.StatusOK},
		{"normal TV", 1, 2, http.StatusOK},
		{"movie zeros", 0, 0, http.StatusOK},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s, id := seedOneTitleRip(t)
			rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept-title", map[string]interface{}{
				"title_name": "ep_t00.mkv",
				"index":      0,
				"tv_season":  tt.season,
				"tv_episode": tt.episode,
			})
			if rec.Code != tt.wantStatus {
				t.Fatalf("status = %d, want %d (%s)", rec.Code, tt.wantStatus, rec.Body.String())
			}
			if tt.wantStatus == http.StatusBadRequest {
				// Title must not have been consumed.
				s.mu.Lock()
				st := s.rips[id].Titles[0].State
				s.mu.Unlock()
				if st != "pending" {
					t.Errorf("rejected accept left title in %q, want pending", st)
				}
			}
		})
	}
}

// A valid TV accept still lands in the Season dir layout (M8 must not break
// existing TV handling).
func TestAcceptTitleValidTVMovesToSeasonDir(t *testing.T) {
	s, id := seedOneTitleRip(t)
	moviesDir := s.cfg.MoviesDir
	rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept-title", map[string]interface{}{
		"title_name": "ep_t00.mkv",
		"index":      0,
		"tv_season":  1,
		"tv_episode": 1,
	})
	if rec.Code != http.StatusOK {
		t.Fatalf("accept-title = %d %s", rec.Code, rec.Body.String())
	}
	// NOTE: destination keeps the source's .mkv in the name AND gains the
	// format's .mkv — pre-existing MoveTitleFile naming, unchanged by M8.
	want := filepath.Join(moviesDir, "KLOVN", "Season 1", "KLOVN - S01E01 - ep_t00.mkv.mkv")
	if _, err := os.Stat(want); err != nil {
		t.Errorf("expected moved file at %s: %v", want, err)
	}
}
