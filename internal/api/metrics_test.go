package api

import (
	"context"
	"encoding/json"
	"net/http"
	"testing"
	"time"

	"riply/internal/rip"
)

// failingRipper reports an error status then closes, mimicking a makemkvcon
// that exits non-zero after the drive read fails.
type failingRipper struct{}

func (failingRipper) GetDiscInfo(_ context.Context, _ string) (rip.DiscInfo, error) {
	return rip.DiscInfo{Title: "Broken"}, nil
}

func (failingRipper) Rip(_ context.Context, _ string, _ string, _ int64) <-chan rip.RipProgress {
	ch := make(chan rip.RipProgress, 2)
	go func() {
		defer close(ch)
		ch <- rip.RipProgress{Status: "ripping", Percent: 10}
		ch <- rip.RipProgress{Status: "error", Message: "disc read error"}
	}()
	return ch
}

func (failingRipper) Eject(_ context.Context, _ string) error { return nil }

func (failingRipper) IsAvailable() bool { return true }

func (failingRipper) Version() string { return "test" }

// M4: the rip-failure branch must bump FailedRips; /api/metrics then shows
// failed_rips: 1 with active_rips back to 0.
func TestFailedRipIncrementsFailedRips(t *testing.T) {
	s, _ := newRipServer(t, failingRipper{})

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "error" }, 5*time.Second); err != nil {
		t.Fatalf("rip never errored: %v", err)
	}
	waitDone(s, id, 2*time.Second)

	rec := doJSON(t, s, http.MethodGet, "/api/metrics", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("metrics status = %d", rec.Code)
	}
	var m map[string]int64
	if err := json.Unmarshal(rec.Body.Bytes(), &m); err != nil {
		t.Fatal(err)
	}
	if m["failed_rips"] != 1 {
		t.Errorf("failed_rips = %d, want 1", m["failed_rips"])
	}
	if m["active_rips"] != 0 {
		t.Errorf("active_rips = %d, want 0", m["active_rips"])
	}
	if m["successful_rips"] != 0 {
		t.Errorf("successful_rips = %d, want 0 for a failed rip", m["successful_rips"])
	}
}

// A successful rip must not touch FailedRips (guards against moving the
// increment into a shared code path).
func TestSuccessfulRipDoesNotIncrementFailedRips(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "complete" }, 5*time.Second); err != nil {
		t.Fatalf("rip never completed: %v", err)
	}
	waitDone(s, id, 2*time.Second)

	rec := doJSON(t, s, http.MethodGet, "/api/metrics", nil)
	var m map[string]int64
	if err := json.Unmarshal(rec.Body.Bytes(), &m); err != nil {
		t.Fatal(err)
	}
	if m["successful_rips"] != 1 || m["failed_rips"] != 0 {
		t.Errorf("metrics = %v, want successful=1 failed=0", m)
	}
}
