package api

import (
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"testing"
)

// Accept-title must act on the clicked row's index, not on whichever pending
// title happens to share the base name. Two titles with identical source_file
// values (possible when MakeMKV output and a user-entered name collide) must
// still resolve unambiguously via the index field. (P1-4)
func TestAcceptTitleUsesIndexNotName(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	// Seed a completed session with two titles sharing one base name. The
	// distinguishing property is file size: row 0 small, row 1 big.
	s.mu.Lock()
	s.ripCount++
	id := s.ripCount
	outDir := t.TempDir()
	for _, spec := range []struct {
		name string
		size int
	}{
		{"title_t00.mkv", 1 << 10},
		{"title_t01.mkv", 1 << 20},
	} {
		if err := os.WriteFile(filepath.Join(outDir, spec.name), make([]byte, spec.size), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	sess := &RipSession{
		ID:        id,
		DrivePath: "/dev/sr0",
		DiscName:  "Inception",
		State:     "complete",
		OutputDir: outDir,
		Done:      make(chan struct{}),
		acceptMu:  &sync.Mutex{},
		Titles: []*RipTitle{
			{SourceFile: "title_t00.mkv", SizeMB: 1, AcceptedName: "Inception", State: "pending"},
			{SourceFile: "title_t01.mkv", SizeMB: 1, AcceptedName: "Inception", State: "pending"},
		},
	}
	close(sess.Done)
	s.rips[id] = sess
	moviesDir := s.cfg.MoviesDir
	s.mu.Unlock()

	// Accept row 1 by index, but send the name of row 0: an index-aware
	// server moves title_t01; a name-matching server would move title_t00.
	idx := 1
	rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept-title", map[string]interface{}{
		"title_name": "title_t00.mkv",
		"index":      idx,
	})
	if rec.Code != http.StatusOK {
		t.Fatalf("accept-title = %d %s", rec.Code, rec.Body.String())
	}

	var moved []string
	err := filepath.WalkDir(moviesDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !d.IsDir() {
			moved = append(moved, d.Name())
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}

	s.mu.Lock()
	states := []string{}
	for _, tt := range s.rips[id].Titles {
		states = append(states, tt.State)
	}
	s.mu.Unlock()

	if len(moved) != 1 || moved[0] != "title_t01.mkv" {
		t.Errorf("moved files = %v, want exactly [title_t01.mkv]", moved)
	}
	want := []string{"pending", "accepted"}
	for i := range states {
		if states[i] != want[i] {
			t.Errorf("title %d state = %q, want %q", i, states[i], want[i])
		}
	}

	// Out-of-range index must be rejected.
	bad := 5
	rec = doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept-title", map[string]interface{}{
		"title_name": "title_t00.mkv",
		"index":      bad,
	})
	if rec.Code != http.StatusNotFound {
		t.Errorf("out-of-range index = %d, want 404", rec.Code)
	}

	// Omitted index falls back to name matching (old cached frontend).
	rec = doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept-title", map[string]interface{}{
		"title_name": "title_t00.mkv",
	})
	if rec.Code != http.StatusOK {
		t.Fatalf("fallback accept-title = %d %s", rec.Code, rec.Body.String())
	}

	s.mu.Lock()
	finalStates := []string{}
	for _, tt := range s.rips[id].Titles {
		finalStates = append(finalStates, tt.State)
	}
	finalState := s.rips[id].State
	s.mu.Unlock()
	if finalStates[0] != "accepted" {
		t.Errorf("fallback name match left row 0 in %q", finalStates[0])
	}
	if finalState != "accepted" {
		t.Errorf("session state after all titles accepted = %q, want accepted", finalState)
	}

}
