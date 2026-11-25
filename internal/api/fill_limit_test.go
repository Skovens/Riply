package api

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"riply/internal/drive"
	"riply/internal/move"
)

func TestOverLimit(t *testing.T) {
	cases := []struct {
		used, need, total uint64
		limit             int
		want              bool
	}{
		{0, 0, 1000, 80, false},          // nothing used
		{700, 0, 1000, 80, false},        // at 70%
		{700, 100, 1000, 80, false},      // exactly 80%: not over
		{700, 101, 1000, 80, true},       // 80.1%: over
		{0, 801, 1000, 80, true},         // need alone pushes over
		{900, 500, 1000, 80, true},       // far over
		{999, 999, 1000, 0, false},       // 0 disables
		{999, 999, 1000, -5, false},      // negative disables
		{0, 0, 0, 80, false},             // unknown total: never block
		{999, 999, 1000, 100, true},      // 199% even vs 100
	}
	for i, c := range cases {
		if got := move.OverLimit(c.used, c.need, c.total, c.limit); got != c.want {
			t.Errorf("case %d: OverLimit(%d,%d,%d,%d) = %v, want %v", i, c.used, c.need, c.total, c.limit, got, c.want)
		}
	}
}

// TestTempFillLimitBlocksRip: usage above the limit must block startRipForPath
// with an explanatory error and publish rip_failed — and never register a session.
func TestTempFillLimitBlocksRip(t *testing.T) {
	origUsage := diskUsageFn
	defer func() { diskUsageFn = origUsage }()
	diskUsageFn = func(string) (uint64, uint64, error) {
		return 1000, 100, nil // 90% used
	}

	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.cfg.TempFillLimitPercent = 80
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	sub := s.sseMgr.Subscribe()
	defer s.sseMgr.Unsubscribe(sub)

	if _, err := s.startRipForPath("/dev/sr0"); err == nil {
		t.Fatal("rip started although temp fill limit exceeded, want block")
	} else if !strings.Contains(err.Error(), "fill limit") {
		t.Errorf("error = %v, want fill-limit explanation", err)
	}

	select {
	case ev := <-sub.Chan():
		if ev.Type != "rip_failed" {
			t.Fatalf("event type = %q, want rip_failed", ev.Type)
		}
		if !strings.Contains(ev.Message, "fill limit") {
			t.Errorf("message = %q, want fill-limit wording", ev.Message)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("no rip_failed event published")
	}

	s.mu.Lock()
	count := s.ripCount
	s.mu.Unlock()
	if count != 0 {
		t.Errorf("ripCount = %d, want 0 (blocked rip must not register)", count)
	}
}

// TestTempFillLimitZeroDisables: limit 0 must never block, whatever usage says.
func TestTempFillLimitZeroDisables(t *testing.T) {
	origUsage := diskUsageFn
	defer func() { diskUsageFn = origUsage }()
	diskUsageFn = func(string) (uint64, uint64, error) {
		return 1000, 1, nil // 99.9% used
	}

	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.cfg.TempFillLimitPercent = 0
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	if _, err := s.startRipForPath("/dev/sr0"); err != nil {
		t.Fatalf("rip blocked despite disabled check: %v", err)
	}
	waitRipsIdle(s, 5*time.Second) // goroutine must finish before TempDir cleanup
}

// TestMoveFillLimitBlocksAccept: an over-limit destination must reject the
// accept with 507, publish rip_failed, and leave the rip output in place.
func TestMoveFillLimitBlocksAccept(t *testing.T) {
	origUsage, origSize := diskUsageFn, dirSizeFn
	defer func() { diskUsageFn, dirSizeFn = origUsage, origSize }()
	diskUsageFn = func(string) (uint64, uint64, error) {
		return 1000, 50, nil // 95% used
	}
	dirSizeFn = func(string) (uint64, error) {
		return 100, nil // would push to 105%
	}

	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.cfg.MoveFillLimitPercent = 95

	if err := s.checkMoveFillLimit("/dev/sr0", s.cfg.MoviesDir, s.cfg.TempDir); err == nil {
		t.Fatal("move not blocked although destination fill limit exceeded")
	}
}

// TestFillLimitUnknownUsageProceeds: an unavailable usage query must never
// block (unknown ≠ full).
func TestFillLimitUnknownUsageProceeds(t *testing.T) {
	origUsage := diskUsageFn
	defer func() { diskUsageFn = origUsage }()
	diskUsageFn = func(string) (uint64, uint64, error) {
		return 0, 0, fmt.Errorf("statfs unavailable")
	}

	s, scanner := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.cfg.TempFillLimitPercent = 80
	scanner.UpdateState("/dev/sr0", drive.StateReady, "DISC")

	if _, err := s.startRipForPath("/dev/sr0"); err != nil {
		t.Fatalf("rip blocked on unknown usage: %v", err)
	}
	waitRipsIdle(s, 5*time.Second) // goroutine must finish before TempDir cleanup
}

