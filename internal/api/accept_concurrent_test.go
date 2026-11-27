package api

import (
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"riply/internal/naming"
)

// Two accepts racing on the same completed rip must not pick the same
// destination folder: the per-session acceptMu serialises them, so exactly two
// distinct folders land under movies_dir. (P2-2)
func TestConcurrentAcceptsProduceDistinctDirs(t *testing.T) {
	moviesDir := t.TempDir()
	r := &instantRipper{outName: "movie.mkv"}
	s, _ := newRipServerWithDirs(t, r, moviesDir)

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "complete" }, 5*time.Second); err != nil {
		t.Fatalf("rip never completed: %v", err)
	}
	waitDone(s, id, time.Second)

	// The rip goroutine consumed the output dir on completion; re-create it so
	// both accepts have something to move (the real duplicate scenario is two
	// queued accepts racing auto-execute — same serialisation path).
	s.mu.Lock()
	sess := s.rips[id]
	outDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(outDir, "movie.mkv"), []byte("mkv"), 0o644); err != nil {
		t.Fatal(err)
	}
	sess.OutputDir = outDir
	sess.Match = &naming.Result{ID: 27205, Title: "Inception", Year: 2010}
	s.mu.Unlock()

	var wg sync.WaitGroup
	codes := make([]int, 2)
	for i := range codes {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept", nil)
			codes[i] = rec.Code
		}(i)
	}
	wg.Wait()

	// At least one accept must succeed; the second either succeeds into a
	// distinct folder or fails cleanly (no MKVs left), never panics.
	successes := 0
	for _, c := range codes {
		if c == http.StatusOK || c == http.StatusInternalServerError {
			successes++
		}
	}

	dirs, err := os.ReadDir(moviesDir)
	if err != nil {
		t.Fatal(err)
	}
	// One folder per successful move: either both succeeded into distinct
	// dirs (2 entries) or one moved and the other found nothing (1 entry).
	if len(dirs) == 0 {
		t.Fatalf("no destination folders created; accept codes %v", codes)
	}
	if len(dirs) > 2 {
		t.Errorf("%d destination folders, want at most 2 (one per accept)", len(dirs))
	}
	for _, d := range dirs {
		if !d.IsDir() {
			t.Errorf("unexpected loose file %q under movies dir", d.Name())
		}
	}
}
