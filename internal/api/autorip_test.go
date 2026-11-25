package api

import (
	"encoding/json"
	"fmt"
	"net/http"
	"testing"
	"time"

	"riply/internal/drive"
	"riply/internal/naming"
)

func TestToggleAutoRip(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	rec := doJSON(t, s, http.MethodPut, "/api/drives/0/autorip", map[string]any{"enabled": false})
	if rec.Code != http.StatusOK {
		t.Fatalf("toggle autorip = %d %s", rec.Code, rec.Body.String())
	}
	var out map[string]string
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatal(err)
	}
	if out["status"] != "ok" || out["autorip"] != "false" {
		t.Fatalf("unexpected response: %v", out)
	}

	d, ok := scanner.Snapshot("/dev/sr0")
	if !ok {
		t.Fatal("drive missing")
	}
	if d.AutoRipEnabled {
		t.Errorf("AutoRipEnabled = true, want false")
	}
}

func TestAutoEjectAfterQueuedAccept(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "Inception")

	s.mu.Lock()
	s.namingMatches["/dev/sr0"] = naming.Result{Title: "Inception", Year: 2010, IsTV: false}
	s.mu.Unlock()

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/accept", map[string]any{})
	if rec.Code != http.StatusOK {
		t.Fatalf("drive accept = %d %s", rec.Code, rec.Body.String())
	}

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "accepted" }, 5*time.Second); err != nil {
		t.Fatalf("session never accepted: %v", err)
	}
	waitDone(s, id, 5*time.Second)

	d, _ := scanner.Snapshot("/dev/sr0")
	if d.State != drive.StateNoDisc {
		t.Errorf("drive state = %q, want no_disc after auto-eject", d.State)
	}

	s.mu.Lock()
	guard := s.lastAutoEjectedDisc["/dev/sr0"]
	s.mu.Unlock()
	if guard != "Inception" {
		t.Errorf("lastAutoEjectedDisc = %q, want Inception", guard)
	}
}

func TestAutoRipDisabledSkipsEject(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "Inception")
	scanner.SetAutoRip("/dev/sr0", false)

	s.mu.Lock()
	s.namingMatches["/dev/sr0"] = naming.Result{Title: "Inception", Year: 2010, IsTV: false}
	s.mu.Unlock()

	rec := doJSON(t, s, http.MethodPost, "/api/drives/0/accept", map[string]any{})
	if rec.Code != http.StatusOK {
		t.Fatalf("drive accept = %d %s", rec.Code, rec.Body.String())
	}

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "accepted" }, 5*time.Second); err != nil {
		t.Fatalf("session never accepted: %v", err)
	}
	waitDone(s, id, 5*time.Second)

	d, _ := scanner.Snapshot("/dev/sr0")
	if d.State == drive.StateNoDisc {
		t.Errorf("drive state = no_disc, want unchanged because autorip is disabled")
	}
}

func TestMaybeStartAutoRip(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")
	s.manualRipStarted.Store(true)

	s.MaybeStartAutoRip("/dev/sr0", "DISC")

	if err := waitForActiveRip(s, 5*time.Second); err != nil {
		t.Fatalf("auto-rip did not start: %v", err)
	}
}

func TestMaybeStartAutoRipDisabled(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")
	scanner.SetAutoRip("/dev/sr0", false)
	s.manualRipStarted.Store(true)

	s.MaybeStartAutoRip("/dev/sr0", "DISC")

	time.Sleep(50 * time.Millisecond)
	s.mu.Lock()
	count := s.ripCount
	s.mu.Unlock()
	if count != 0 {
		t.Errorf("ripCount = %d, want 0 when autorip disabled", count)
	}
}

func TestMaybeStartAutoRipSameDiscGuard(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	s.mu.Lock()
	s.lastAutoEjectedDisc["/dev/sr0"] = "DISC"
	s.mu.Unlock()
	s.manualRipStarted.Store(true)

	s.MaybeStartAutoRip("/dev/sr0", "DISC")

	time.Sleep(50 * time.Millisecond)
	s.mu.Lock()
	count := s.ripCount
	s.mu.Unlock()
	if count != 0 {
		t.Errorf("ripCount = %d, want 0 for same-disc guard", count)
	}
}

func waitForActiveRip(s *Server, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		s.mu.Lock()
		active := false
		for _, sess := range s.rips {
			if sess.State == string(StateRipping) || sess.State == string(StateQueued) {
				active = true
				break
			}
		}
		s.mu.Unlock()
		if active {
			return nil
		}
		time.Sleep(5 * time.Millisecond)
	}
	return fmt.Errorf("timeout waiting for active rip")
}

func TestMaybeStartAutoRipBlockedBeforeManualRip(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	// manualRipStarted is false (zero value) — autorip must not start.
	s.MaybeStartAutoRip("/dev/sr0", "DISC")

	time.Sleep(50 * time.Millisecond)
	s.mu.Lock()
	count := s.ripCount
	s.mu.Unlock()
	if count != 0 {
		t.Errorf("ripCount = %d, want 0 before manual rip", count)
	}
}

func TestManualRipEnablesAutorip(t *testing.T) {
	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	// First: manual rip via the HTTP endpoint.
	id := startRipViaAPI(t, s)
	waitDone(s, id, 5*time.Second)

	// Now: simulate a new disc insert with autorip enabled.
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC2")
	s.MaybeStartAutoRip("/dev/sr0", "DISC2")

	// instantRipper completes within milliseconds, so poll the rip counter
	// instead of the transient "ripping" state.
	deadline := time.Now().Add(5 * time.Second)
	for {
		s.mu.Lock()
		count := s.ripCount
		s.mu.Unlock()
		if count >= 2 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("autorip did not start after manual rip: ripCount = %d, want 2", count)
		}
		time.Sleep(10 * time.Millisecond)
	}
}
