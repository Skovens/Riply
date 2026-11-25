package api

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"riply/internal/config"
	"riply/internal/drive"
	"riply/internal/metrics"
	"riply/internal/naming"
	"riply/internal/rip"
	"riply/internal/sse"
)

func itoa(i int) string { return strconv.Itoa(i) }

// instantRipper produces one MKV into the session's outDir, optionally waits
// on a block channel, then completes — the fastest realistic path into the
// accept/undo race window.
type instantRipper struct {
	outName string
	block   chan struct{}
}

func (r *instantRipper) GetDiscInfo(_ context.Context, _ string) (rip.DiscInfo, error) {
	return rip.DiscInfo{Title: "Inception"}, nil
}

func (r *instantRipper) Rip(ctx context.Context, _ string, outDir string, _ int64) <-chan rip.RipProgress {
	ch := make(chan rip.RipProgress, 4)
	go func() {
		defer close(ch)
		_ = os.MkdirAll(outDir, 0o755)
		_ = os.WriteFile(filepath.Join(outDir, r.outName), []byte("mkv"), 0o644)
		if r.block != nil {
			select {
			case <-ctx.Done():
				return
			case <-r.block:
			}
		}
		select {
		case <-ctx.Done():
		case ch <- rip.RipProgress{Status: "complete", Percent: 100}:
		}
	}()
	return ch
}

func (r *instantRipper) Eject(_ context.Context, _ string) error { return nil }
func (r *instantRipper) Version() string                         { return "test" }
func (r *instantRipper) IsAvailable() bool                       { return true }

func newRipServer(t *testing.T, ripper rip.Ripper) (*Server, *drive.Scanner) {
	t.Helper()
	scanner := drive.New()
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DriveDiscovered, Path: "/dev/sr0"})
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DiscInserted, Path: "/dev/sr0"})

	cfg := config.Default()
	cfg.TempDir = t.TempDir()
	cfg.MoviesDir = t.TempDir()
	s := New(cfg, filepath.Join(t.TempDir(), "config.toml"), scanner, ripper, nil, sse.New(), &metrics.Metrics{}, nil)
	return s, scanner
}

func newRipServerWithDirs(t *testing.T, ripper rip.Ripper, moviesDir string) (*Server, *drive.Scanner) {
	t.Helper()
	scanner := drive.New()
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DriveDiscovered, Path: "/dev/sr0"})
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DiscInserted, Path: "/dev/sr0"})

	cfg := config.Default()
	cfg.TempDir = t.TempDir()
	cfg.MoviesDir = moviesDir
	s := New(cfg, filepath.Join(t.TempDir(), "config.toml"), scanner, ripper, nil, sse.New(), &metrics.Metrics{}, nil)
	return s, scanner
}

func doJSON(t *testing.T, s *Server, method, path string, body interface{}) *httptest.ResponseRecorder {
	t.Helper()
	var payload io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		payload = strings.NewReader(string(b))
	} else {
		payload = strings.NewReader("{}")
	}
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	req := httptest.NewRequest(method, path, payload)
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	return rec
}

// startRipViaAPI kicks off a rip on drive index 0 and returns the session ID.
func startRipViaAPI(t *testing.T, s *Server) int {
	t.Helper()
	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/start", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("start rip = %d %s", rec.Code, rec.Body.String())
	}
	var out struct {
		RipID int `json:"rip_id"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatal(err)
	}
	return out.RipID
}

// A completed rip must never be demoted to "ripping": the handler rejects
// undo on a non-queued session with 409, and the goroutine publishes its
// terminal state under s.mu before acting on any still-queued accept. (P1-3)
func TestUndoAcceptAfterCompletionKeepsTerminalState(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool {
		return st == "complete" || st == "error"
	}, 5*time.Second); err != nil {
		t.Fatalf("rip never completed: %v", err)
	}
	waitDone(s, id, time.Second)

	// Undo on a completed (non-queued) session must be rejected.
	rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/undo-accept", nil)
	if rec.Code != http.StatusConflict {
		t.Fatalf("undo on completed rip = %d %s, want 409", rec.Code, rec.Body.String())
	}

	s.mu.Lock()
	state := s.rips[id].State
	s.mu.Unlock()
	if state == "ripping" {
		t.Errorf("undo resurrected completed rip as %q", state)
	}
}

// The real race: a queued accept being undone while the rip goroutine is
// finishing. Whichever order the two take, the session must end in a sane
// state — never stranded "ripping" after completion, and the files stay put
// unless an accept actually executed (undo means no move).
func TestUndoAcceptRacesCompletion(t *testing.T) {
	moviesDir := t.TempDir()
	r := &instantRipper{outName: "movie.mkv", block: make(chan struct{})}
	s, _ := newRipServerWithDirs(t, r, moviesDir)

	id := startRipViaAPI(t, s)
	// Queue an accept while the rip is still running.
	if err := waitState(s, id, func(st string) bool { return st == "ripping" }, 5*time.Second); err != nil {
		t.Fatalf("rip never started: %v", err)
	}
	rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept", map[string]string{"title": "Inception"})
	if rec.Code != http.StatusAccepted {
		t.Fatalf("accept = %d %s", rec.Code, rec.Body.String())
	}

	// Undo immediately — racing the goroutine's completion path.
	rec = doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/undo-accept", nil)
	if rec.Code != http.StatusOK && rec.Code != http.StatusConflict {
		t.Fatalf("undo during completion race = %d %s", rec.Code, rec.Body.String())
	}

	// Let the rip finish after the undo has already cleared PendingAccept.
	close(r.block)
	waitDone(s, id, 5*time.Second)
	time.Sleep(50 * time.Millisecond)

	s.mu.Lock()
	state := s.rips[id].State
	pending := s.rips[id].PendingAccept
	s.mu.Unlock()

	if pending != nil {
		t.Error("undo left a PendingAccept behind")
	}
	switch state {
	case "complete", "error":
		// The undo won or lost the race; either way the session reached a
		// terminal state and no move happened.
	case "accepted":
		t.Error("undo must not execute the accept; files were moved")
	case "queued", "ripping":
		t.Errorf("session stranded in %q after completion race", state)
	}

	if entries, _ := os.ReadDir(moviesDir); len(entries) != 0 {
		t.Errorf("undo raced into a move: %d entries under movies dir", len(entries))
	}
}

// The normal path must still work: undo while the rip goroutine is genuinely
// running reverts "queued" → "ripping".
func TestUndoAcceptWhileRipRunningRevertsToRipping(t *testing.T) {
	slow := &blockingRipper{outName: "movie.mkv", release: make(chan struct{})}
	s, _ := newRipServer(t, slow)

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "ripping" }, 5*time.Second); err != nil {
		t.Fatalf("rip never started: %v", err)
	}

	// Queue an accept via the API (state ripping → queued).
	rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept", map[string]string{"title": "Inception"})
	if rec.Code != http.StatusAccepted {
		t.Fatalf("accept = %d %s", rec.Code, rec.Body.String())
	}

	rec = doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/undo-accept", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("undo-accept = %d %s", rec.Code, rec.Body.String())
	}

	s.mu.Lock()
	state := s.rips[id].State
	s.mu.Unlock()
	if state != "ripping" {
		t.Errorf("state after undo while running = %q, want ripping", state)
	}

	close(slow.release) // let the fake finish so the goroutine exits
}

func waitState(s *Server, id int, pred func(string) bool, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		s.mu.Lock()
		sess, ok := s.rips[id]
		var state string
		if ok {
			state = sess.State
		}
		s.mu.Unlock()
		if !ok {
			return errRipMissing{}
		}
		if pred(state) {
			return nil
		}
		time.Sleep(5 * time.Millisecond)
	}
	return fmt.Errorf("timeout waiting for state")
}

// waitDone blocks until the session's Done channel closes or timeout.
func waitDone(s *Server, id int, timeout time.Duration) {
	s.mu.Lock()
	done := s.rips[id].Done
	s.mu.Unlock()
	select {
	case <-done:
	case <-time.After(timeout):
	}
}

type errRipMissing struct{}

func (errRipMissing) Error() string { return "rip missing" }

// blockingRipper produces one MKV then blocks until release closes.
type blockingRipper struct {
	outName string
	release chan struct{}
}

func (r *blockingRipper) GetDiscInfo(_ context.Context, _ string) (rip.DiscInfo, error) {
	return rip.DiscInfo{Title: "Inception"}, nil
}

func (r *blockingRipper) Rip(ctx context.Context, _ string, outDir string, _ int64) <-chan rip.RipProgress {
	ch := make(chan rip.RipProgress, 4)
	go func() {
		defer close(ch)
		_ = os.MkdirAll(outDir, 0o755)
		_ = os.WriteFile(filepath.Join(outDir, r.outName), []byte("mkv"), 0o644)
		ch <- rip.RipProgress{Status: "ripping", Percent: 10}
		select {
		case <-ctx.Done():
		case <-r.release:
		}
	}()
	return ch
}

func (r *blockingRipper) Eject(_ context.Context, _ string) error { return nil }
func (r *blockingRipper) Version() string                         { return "test" }
func (r *blockingRipper) IsAvailable() bool                       { return true }

var _ = naming.ParseTitle // keep import tidy if unused elsewhere

// recordingRipper is a minimal fake that records SetMinTitleLength calls,
// exercising the optional MinLengthSetter path in handlePutConfig (N2).
type recordingRipper struct {
	mu      sync.Mutex
	minLen  int
	hadCall bool
}

func (r *recordingRipper) GetDiscInfo(_ context.Context, _ string) (rip.DiscInfo, error) {
	return rip.DiscInfo{}, nil
}
func (r *recordingRipper) Rip(_ context.Context, _ string, _ string, _ int64) <-chan rip.RipProgress {
	ch := make(chan rip.RipProgress)
	close(ch)
	return ch
}
func (r *recordingRipper) Eject(_ context.Context, _ string) error { return nil }
func (r *recordingRipper) Version() string                         { return "fake" }
func (r *recordingRipper) IsAvailable() bool                       { return true }
func (r *recordingRipper) SetMinTitleLength(seconds int) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.minLen = seconds
	r.hadCall = true
}
func (r *recordingRipper) gotMinLen() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	if !r.hadCall {
		return -1
	}
	return r.minLen
}
