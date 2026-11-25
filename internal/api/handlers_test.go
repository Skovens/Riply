package api

import (
	"context"
	"encoding/json"
	"net/http"
	"strings"
	"testing"
	"time"

	"riply/internal/drive"
	"riply/internal/naming"
)

// unavailableRipper always reports unavailable (simulates missing MakeMKV).
type unavailableRipper struct{ instantRipper }

func (r *unavailableRipper) IsAvailable() bool { return false }

// --- handleEject ---

type ejectFailRipper struct {
	instantRipper
}

func (r *ejectFailRipper) Eject(_ context.Context, _ string) error {
	return context.DeadlineExceeded
}

func TestHandleEjectSuccess(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/eject", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("eject = %d %s", rec.Code, rec.Body.String())
	}
	d, _ := scanner.Snapshot("/dev/sr0")
	if d.State != drive.StateNoDisc {
		t.Errorf("state = %q, want no_disc", d.State)
	}
}

func TestHandleEjectUnavailableReturns503(t *testing.T) {
	s, _ := newRipServer(t, &unavailableRipper{})
	scanner := s.scanner
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DriveDiscovered, Path: "/dev/sr0"})

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/eject", nil)
	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("eject = %d, want 503", rec.Code)
	}
}

func TestHandleEjectUnknownDriveReturns404(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	rec := doJSON(t, s, http.MethodPost, "/api/drives/7/eject", nil)
	if rec.Code != http.StatusNotFound {
		t.Fatalf("eject = %d, want 404", rec.Code)
	}
}

func TestHandleEjectCancelsActiveRip(t *testing.T) {
	block := make(chan struct{})
	defer close(block)
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv", block: block})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	id := startRipViaAPI(t, s)

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/eject", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("eject = %d %s", rec.Code, rec.Body.String())
	}
	s.mu.Lock()
	state := s.rips[id].State
	s.mu.Unlock()
	if state != string(StateCancelled) {
		t.Errorf("session state = %q, want cancelled", state)
	}
}

// --- handleCancelRip ---

func TestHandleCancelRipLifecycle(t *testing.T) {
	block := make(chan struct{})
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv", block: block})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	if rec := doJSON(t, s, http.MethodPost, "/api/rips/abc/cancel", nil); rec.Code != http.StatusBadRequest {
		t.Fatalf("invalid id = %d, want 400", rec.Code)
	}
	if rec := doJSON(t, s, http.MethodPost, "/api/rips/99/cancel", nil); rec.Code != http.StatusNotFound {
		t.Fatalf("missing id = %d, want 404", rec.Code)
	}

	id := startRipViaAPI(t, s)
	if rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/cancel", nil); rec.Code != http.StatusOK {
		t.Fatalf("cancel = %d %s", rec.Code, rec.Body.String())
	}

	// Cancelling twice: the session is terminal now → 409.
	if rec := doJSON(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/cancel", nil); rec.Code != http.StatusConflict {
		t.Fatalf("second cancel = %d, want 409", rec.Code)
	}
	close(block)
	waitRipsIdle(s, 5*time.Second) // before TempDir cleanup
}

// --- handleSetName ---

func TestHandleSetName(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	if rec := doJSON(t, s, http.MethodPut, "/api/drives/0/name", map[string]any{}); rec.Code != http.StatusBadRequest {
		t.Fatalf("empty body = %d, want 400", rec.Code)
	}

	rec := doJSON(t, s, http.MethodPut, "/api/drives/0/name", map[string]any{
		"title": "The Mask", "year": 1994, "tmdb_id": 27205,
	})
	if rec.Code != http.StatusOK {
		t.Fatalf("set name = %d %s", rec.Code, rec.Body.String())
	}
	s.mu.Lock()
	match := s.namingMatches["/dev/sr0"]
	s.mu.Unlock()
	if match.Title != "The Mask" || match.Year != 1994 || match.ID != 27205 {
		t.Errorf("namingMatches = %+v, want The Mask/1994/27205", match)
	}
}

// --- handleDriveUndoAccept ---

func TestHandleDriveUndoAcceptClearsAccept(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.mu.Lock()
	s.driveAccepts["/dev/sr0"] = driveAccept{result: naming.Result{Title: "X"}}
	s.mu.Unlock()

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/undo-accept", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("undo-accept = %d %s", rec.Code, rec.Body.String())
	}
	s.mu.Lock()
	_, ok := s.driveAccepts["/dev/sr0"]
	s.mu.Unlock()
	if ok {
		t.Error("driveAccepts entry survived undo-accept")
	}
}

// --- handleToggleAutoNaming / handleToggleAutoSequence ---

func TestHandleToggles(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	if rec := doJSON(t, s, http.MethodPut, "/api/drives/0/auto-naming", map[string]any{"enabled": false}); rec.Code != http.StatusOK {
		t.Fatalf("auto-naming = %d %s", rec.Code, rec.Body.String())
	}
	if rec := doJSON(t, s, http.MethodPut, "/api/drives/0/auto-sequence", map[string]any{"enabled": false}); rec.Code != http.StatusOK {
		t.Fatalf("auto-sequence = %d %s", rec.Code, rec.Body.String())
	}

	d, _ := scanner.Snapshot("/dev/sr0")
	if d.AutoNamingEnabled || d.AutoSequenceEnabled {
		t.Errorf("toggles not applied: naming=%v sequence=%v", d.AutoNamingEnabled, d.AutoSequenceEnabled)
	}

	// Unknown drive → 404.
	if rec := doJSON(t, s, http.MethodPut, "/api/drives/5/auto-naming", map[string]any{"enabled": true}); rec.Code != http.StatusNotFound {
		t.Fatalf("unknown drive = %d, want 404", rec.Code)
	}
}

// --- handleStartRip failure paths ---

func TestHandleStartRipFailurePaths(t *testing.T) {
	block := make(chan struct{})
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv", block: block})

	if rec := doJSON(t, s, http.MethodPost, "/api/drives/9/start", nil); rec.Code != http.StatusNotFound {
		t.Fatalf("unknown drive = %d, want 404", rec.Code)
	}

	// newRipServer leaves the drive Ready (disc inserted, no name).
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")
	if rec := doJSON(t, s, http.MethodPost, "/api/drives/0/start", nil); rec.Code != http.StatusOK {
		t.Fatalf("first start = %d %s, want 200", rec.Code, rec.Body.String())
	}
	if rec := doJSON(t, s, http.MethodPost, "/api/drives/0/start", nil); rec.Code != http.StatusConflict {
		t.Fatalf("busy drive = %d, want 409", rec.Code)
	}
	close(block)
	waitRipsIdle(s, 5*time.Second) // before TempDir cleanup
}

// --- handleListRips shape ---

func TestHandleListRipsShape(t *testing.T) {
	block := make(chan struct{})
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv", block: block})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	id1 := startRipViaAPI(t, s)

	rec := doJSON(t, s, http.MethodGet, "/api/rips", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("list = %d %s", rec.Code, rec.Body.String())
	}
	var out []map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v (%s)", err, rec.Body.String())
	}
	if len(out) != 1 {
		t.Fatalf("len = %d, want 1 active rip", len(out))
	}
	if got := out[0]["id"].(float64); int(got) != id1 {
		t.Errorf("id = %v, want %d", out[0]["id"], id1)
	}
	if out[0]["state"] != string(StateRipping) {
		t.Errorf("state = %v, want ripping", out[0]["state"])
	}

	// Accepted rips are hidden from the list.
	s.mu.Lock()
	s.rips[id1].State = string(StateAccepted)
	s.mu.Unlock()
	rec = doJSON(t, s, http.MethodGet, "/api/rips", nil)
	out = nil
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(out) != 0 {
		t.Errorf("accepted rip listed: %v", out)
	}
	close(block)
	waitRipsIdle(s, 5*time.Second) // before TempDir cleanup
}

// --- handleGetConfig ---

func TestHandleGetConfigRedactsAPIKey(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.cfg.TMDBKey = "secret-key"

	rec := doJSON(t, s, http.MethodGet, "/api/config", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("config = %d %s", rec.Code, rec.Body.String())
	}
	if strings.Contains(rec.Body.String(), "secret-key") {
		t.Error("GET /api/config leaked the API key")
	}
	var cfg map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &cfg); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if _, ok := cfg["movies_dir"]; !ok {
		t.Error("config payload missing movies_dir")
	}
}
