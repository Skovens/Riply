package drive

import (
	"context"
	"sync/atomic"
	"testing"
	"time"
)

// withFakeProbes swaps the three poller seams and restores them on cleanup.
func withFakeProbes(t *testing.T, files []driveFile, status func(string) mediaState, size func(string) (int64, bool)) {
	t.Helper()
	origFiles, origStatus, origSize := findDriveFiles, probeMediaStatus, probeMediaSize
	findDriveFiles = func() ([]driveFile, error) { return files, nil }
	probeMediaStatus = status
	probeMediaSize = size
	t.Cleanup(func() { findDriveFiles, probeMediaStatus, probeMediaSize = origFiles, origStatus, origSize })
}

// collect drains eventCh until quiet (no event within perTick).
func collect(t *testing.T, ch chan DiscEvent, perTick time.Duration) []DiscEvent {
	t.Helper()
	var out []DiscEvent
	deadline := time.After(2 * perTick)
	for {
		select {
		case e, ok := <-ch:
			if !ok {
				return out
			}
			out = append(out, e)
		case <-deadline:
			return out
		}
	}
}

// M5: eject+insert inside one poll window (presence never flaps) but the
// media capacity changes → the poller must emit a DiscInserted so the drive
// re-resolves instead of keeping a stale-named ready card.
func TestPollerDetectsSwapWithinOneWindow(t *testing.T) {
	disc := driveFile{Path: "/dev/sr0"}
	sector := int64(0)
	withFakeProbes(t,
		[]driveFile{disc},
		func(string) mediaState { return mediaPresent },
		func(string) (int64, bool) { return sector, true },
	)

	p := NewPoller()
	ch := make(chan DiscEvent, 16)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	defer p.Stop()

	// Tick 1: drive discovered + disc inserted (disc A, 1 TiB-ish sectors).
	sector = 1_900_000_000
	p.scan(ctx, ch)
	events := collect(t, ch, 100*time.Millisecond)
	var discovered, inserted int
	for _, e := range events {
		switch e.EventType {
		case DriveDiscovered:
			discovered++
		case DiscInserted:
			inserted++
		}
	}
	if discovered != 1 || inserted != 1 {
		t.Fatalf("tick1 events = %v, want 1 discovered + 1 inserted", events)
	}

	// Tick 2: same presence, different capacity = swap happened between ticks.
	sector = 3_800_000_000
	p.scan(ctx, ch)
	events = collect(t, ch, 100*time.Millisecond)
	if len(events) != 1 || events[0].EventType != DiscInserted {
		t.Fatalf("after swap events = %v, want exactly 1 DiscInserted", events)
	}

	// Tick 3: unchanged → no duplicate event.
	p.scan(ctx, ch)
	if events := collect(t, ch, 100*time.Millisecond); len(events) != 0 {
		t.Fatalf("steady-state events = %v, want none", events)
	}
}

// A plain eject→insert (presence flap) still emits eject then insert, and the
// swap detector does not double-fire afterwards.
func TestPollerPlainEjectInsertFlow(t *testing.T) {
	disc := driveFile{Path: "/dev/sr0"}
	present := false
	sector := int64(0)
	withFakeProbes(t,
		[]driveFile{disc},
		func(string) mediaState {
			if present {
				return mediaPresent
			}
			return mediaAbsent
		},
		func(string) (int64, bool) { return sector, true },
	)

	p := NewPoller()
	ch := make(chan DiscEvent, 16)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	defer p.Stop()

	present, sector = true, 1_000
	p.scan(ctx, ch) // discovered + inserted
	collect(t, ch, 50*time.Millisecond)

	// Eject.
	present = false
	p.scan(ctx, ch)
	events := collect(t, ch, 50*time.Millisecond)
	if len(events) != 1 || events[0].EventType != DiscEjected {
		t.Fatalf("eject events = %v, want 1 DiscEjected", events)
	}

	// Insert different disc.
	present, sector = true, 2_000
	p.scan(ctx, ch)
	events = collect(t, ch, 50*time.Millisecond)
	if len(events) != 1 || events[0].EventType != DiscInserted {
		t.Fatalf("insert events = %v, want 1 DiscInserted", events)
	}
	// No swap double-fire next tick.
	p.scan(ctx, ch)
	if events := collect(t, ch, 50*time.Millisecond); len(events) != 0 {
		t.Fatalf("post-insert events = %v, want none", events)
	}
}

// M5: the API eject path writes UpdateState(StateNoDisc) directly (handleEject)
// before the poller's DiscEjected tick lands — the NoDisc write itself must
// clear presence so the rip consumer's late Complete (the 15s cancel-wait
// timeout makes this window real) is dropped.
func TestUpdateStateNoDiscThenLateCompleteDropped(t *testing.T) {
	s := New()
	s.ApplyEvent(DiscEvent{EventType: DriveDiscovered, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	s.UpdateState("/dev/sr0", StateRipping, "Disc")

	// handleEject's write, pre-poller.
	s.UpdateState("/dev/sr0", StateNoDisc, "")
	d, _ := s.Snapshot("/dev/sr0")
	if d.HasDisc {
		t.Fatal("HasDisc still true after UpdateState(StateNoDisc)")
	}

	s.UpdateState("/dev/sr0", StateComplete, "Ghost Disc")
	d, _ = s.Snapshot("/dev/sr0")
	if d.State != StateNoDisc {
		t.Errorf("state = %s, want no_disc (late complete dropped)", d.State)
	}
}

// M5: a hotswap insert while ripping must not demote the drive via
// resolveDiscName's loading write either — BeginLoading refuses while ripping.
func TestBeginLoadingRefusesWhileRipping(t *testing.T) {
	s := New()
	s.ApplyEvent(DiscEvent{EventType: DriveDiscovered, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	s.UpdateState("/dev/sr0", StateRipping, "Disc")

	if s.BeginLoading("/dev/sr0") {
		t.Fatal("BeginLoading accepted while ripping, want refusal")
	}
	d, _ := s.Snapshot("/dev/sr0")
	if d.State != StateRipping {
		t.Errorf("state = %s, want ripping kept", d.State)
	}

	// And CommitReady refuses to land ready on a ripping drive too.
	gen := s.InsertGeneration("/dev/sr0")
	if s.CommitReady("/dev/sr0", "Hotswap Disc", gen) {
		t.Fatal("CommitReady landed on ripping drive, want drop")
	}

	// Normal (non-ripping, disc present) drive still commits.
	s.ApplyEvent(DiscEvent{EventType: DiscEjected, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	gen2 := s.InsertGeneration("/dev/sr0")
	if !s.BeginLoading("/dev/sr0") {
		t.Fatal("BeginLoading refused on empty-state drive, want success")
	}
	if !s.CommitReady("/dev/sr0", "Fresh Disc", gen2) {
		t.Fatal("CommitReady refused on loading drive, want success")
	}
}

// M5 (state machine): DiscInserted while the drive is Ready (stale name) must
// clear the stale disc name and re-arm resolution — not be dropped.
func TestHandleEventInsertWhileReadyClearsStaleName(t *testing.T) {
	s := New()
	s.ApplyEvent(DiscEvent{EventType: DriveDiscovered, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	s.UpdateState("/dev/sr0", StateReady, "Old Disc")

	// Hotswap insert arrives while still Ready with the old name.
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})

	d, ok := s.Snapshot("/dev/sr0")
	if !ok {
		t.Fatal("drive missing")
	}
	if d.DiscName != "" {
		t.Errorf("stale disc name kept: %q, want cleared", d.DiscName)
	}
	if !d.HasDisc {
		t.Error("HasDisc = false, want true after insert")
	}
}

// M5: DiscInserted while ripping must not demote the drive mid-rip.
func TestHandleEventInsertWhileRippingKeepsState(t *testing.T) {
	s := New()
	s.ApplyEvent(DiscEvent{EventType: DriveDiscovered, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	s.UpdateState("/dev/sr0", StateRipping, "Rip Disc")

	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})

	d, _ := s.Snapshot("/dev/sr0")
	if d.State != StateRipping {
		t.Errorf("state = %s during rip, want ripping kept", d.State)
	}
}

// M5: the rip consumer's late Complete write onto a drive the poller already
// flipped to no_disc (eject during rip) must be dropped, not resurrect a
// phantom complete card.
func TestUpdateStateCompleteDroppedWhenNoDisc(t *testing.T) {
	s := New()
	s.ApplyEvent(DiscEvent{EventType: DriveDiscovered, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	s.UpdateState("/dev/sr0", StateRipping, "Disc")

	// Poller ejects mid-rip.
	s.ApplyEvent(DiscEvent{EventType: DiscEjected, Path: "/dev/sr0"})

	// Rip consumer finishes and writes Complete — must be refused.
	s.UpdateState("/dev/sr0", StateComplete, "Ghost Disc")

	d, _ := s.Snapshot("/dev/sr0")
	if d.State != StateNoDisc {
		t.Errorf("state = %s, want no_disc (complete dropped)", d.State)
	}
	if d.DiscName != "" {
		t.Errorf("disc name = %q, want empty on empty drive", d.DiscName)
	}
}

// Normal completion (disc still present) still lands.
func TestUpdateStateCompleteLandsWithDiscPresent(t *testing.T) {
	s := New()
	s.ApplyEvent(DiscEvent{EventType: DriveDiscovered, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	s.UpdateState("/dev/sr0", StateRipping, "Disc")
	s.UpdateState("/dev/sr0", StateComplete, "Disc")

	d, _ := s.Snapshot("/dev/sr0")
	if d.State != StateComplete {
		t.Errorf("state = %s, want complete", d.State)
	}
}

// M6: CommitReady lands only while generation + presence match; an eject
// during the (simulated) resolve drops the stale commit.
func TestCommitReadyCASSemantics(t *testing.T) {
	s := New()
	s.ApplyEvent(DiscEvent{EventType: DriveDiscovered, Path: "/dev/sr0"})
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	gen := s.InsertGeneration("/dev/sr0")

	// Happy path: commit lands.
	if !s.CommitReady("/dev/sr0", "Good Disc", gen) {
		t.Fatal("first CommitReady dropped, want success")
	}
	d, _ := s.Snapshot("/dev/sr0")
	if d.State != StateReady || d.DiscName != "Good Disc" {
		t.Fatalf("state = %+v, want ready/'Good Disc'", d)
	}

	// Eject races in before a second resolve commits → CAS fails.
	s.ApplyEvent(DiscEvent{EventType: DiscEjected, Path: "/dev/sr0"})
	if s.CommitReady("/dev/sr0", "Stale Disc", gen) {
		t.Fatal("CommitReady succeeded after eject, want drop")
	}
	d, _ = s.Snapshot("/dev/sr0")
	if d.State != StateNoDisc || d.DiscName != "" {
		t.Errorf("state = %+v after dropped commit, want no_disc/''", d)
	}

	// Hotswap (second insert) also invalidates the old generation.
	s.ApplyEvent(DiscEvent{EventType: DiscInserted, Path: "/dev/sr0"})
	if s.CommitReady("/dev/sr0", "Race Disc", gen) {
		t.Fatal("CommitReady succeeded after second insert, want drop")
	}
	// Current generation commits.
	gen2 := s.InsertGeneration("/dev/sr0")
	if gen2 == gen {
		t.Fatal("generation did not bump on second insert")
	}
	if !s.CommitReady("/dev/sr0", "New Disc", gen2) {
		t.Fatal("CommitReady with current gen dropped, want success")
	}

	// Drive removed entirely: no panic, drop.
	s.ApplyEvent(DiscEvent{EventType: DriveRemoved, Path: "/dev/sr0"})
	if s.CommitReady("/dev/sr0", "Ghost", s.InsertGeneration("/dev/sr0")) {
		t.Fatal("CommitReady succeeded on removed drive, want drop")
	}
}

// L11: one drive blocking in a probe (bad-media spin-up) must not delay
// sibling drives' insert detection, and a second tick while that drive is
// still wedged must skip it (no goroutine pile-up) rather than block again.
func TestPollerSlowDriveDoesNotDelaySiblings(t *testing.T) {
	origWait := scanProbeWait
	scanProbeWait = 3 * time.Second
	t.Cleanup(func() { scanProbeWait = origWait })

	slow := driveFile{Path: "/dev/sr0"}
	fast := driveFile{Path: "/dev/sr1"}
	block := make(chan struct{})
	unblocked := false
	unblock := func() {
		if !unblocked {
			unblocked = true
			close(block)
		}
	}
	t.Cleanup(unblock)

	var slowEntered atomic.Bool
	var slowCalls atomic.Int32
	withFakeProbes(t,
		[]driveFile{slow, fast},
		func(path string) mediaState {
			if path == slow.Path {
				slowEntered.Store(true)
				slowCalls.Add(1)
				<-block // wedged probe: never returns until test end
			}
			return mediaPresent
		},
		func(string) (int64, bool) { return 1_000, true },
	)

	p := NewPoller()
	ch := make(chan DiscEvent, 64)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	scanDone := make(chan struct{})
	go func() { p.scan(ctx, ch); close(scanDone) }()

	// Tick 1: the fast drive's insert must be detected while the slow probe
	// is still stuck. Serial probing would make this wait on the wedge.
	deadline := time.After(500 * time.Millisecond)
	seenFastDiscovered, seenFastInserted := false, false
	for !(seenFastDiscovered && seenFastInserted) {
		select {
		case e := <-ch:
			if e.Path != fast.Path {
				t.Fatalf("event for %q arrived while its probe is blocked: %+v", e.Path, e)
			}
			switch e.EventType {
			case DriveDiscovered:
				seenFastDiscovered = true
			case DiscInserted:
				seenFastInserted = true
			}
		case <-deadline:
			t.Fatal("fast drive detection delayed behind the stalled drive")
		}
	}
	if !slowEntered.Load() {
		t.Fatal("slow drive probe never entered; fake is misconfigured")
	}
	// scan must still be waiting on the wedged slow probe (not returned).
	select {
	case <-scanDone:
		t.Fatal("scan returned while a dispatched probe was still blocked")
	default:
	}

	// Tick 2 while slow is wedged: must skip slow (in-flight guard → only
	// fast dispatched) and return promptly, not wait scanProbeWait again.
	t2 := make(chan struct{})
	go func() { p.scan(ctx, ch); close(t2) }()
	select {
	case <-t2:
	case <-time.After(500 * time.Millisecond):
		t.Fatal("second tick blocked on the wedged drive instead of skipping it")
	}
	if n := slowCalls.Load(); n != 1 {
		t.Errorf("slow drive probe launched %d times while wedged, want exactly 1 (no pile-up)", n)
	}

	// Unblock: the still-pending tick-1 probe completes; slow gets observed.
	unblock()
	<-scanDone

	inserted := false
	for _, e := range drain(ch) {
		if e.Path == slow.Path && e.EventType == DiscInserted {
			inserted = true
		}
	}
	if !inserted {
		t.Error("stalled drive never surfaced once its probe returned")
	}
}

// drain reads all currently-buffered events without blocking.
func drain(ch chan DiscEvent) []DiscEvent {
	var out []DiscEvent
	for {
		select {
		case e, ok := <-ch:
			if !ok {
				return out
			}
			out = append(out, e)
		default:
			return out
		}
	}
}

// TestPollerDiscInsertedCarriesMediaSize verifies that a DiscInserted event
// carries the media capacity in bytes (sectors × 512) and SizeKnown=true
// when the size probe succeeds.
func TestPollerDiscInsertedCarriesMediaSize(t *testing.T) {
	const sectors = int64(2048000) // ~1 GiB
	wantBytes := sectors * 512

	withFakeProbes(t,
		[]driveFile{{Path: "/dev/sr0"}},
		func(string) mediaState { return mediaPresent },
		func(string) (int64, bool) { return sectors, true },
	)

	p := NewPoller()
	ch := make(chan DiscEvent, 16)
	ctx := context.Background()

	p.scan(ctx, ch)
	events := collect(t, ch, 100*time.Millisecond)

	var inserted *DiscEvent
	for i := range events {
		if events[i].EventType == DiscInserted {
			inserted = &events[i]
			break
		}
	}
	if inserted == nil {
		t.Fatalf("expected a DiscInserted event, got %v", events)
	}
	if inserted.MediaSize != wantBytes {
		t.Errorf("MediaSize = %d, want %d", inserted.MediaSize, wantBytes)
	}
	if !inserted.SizeKnown {
		t.Error("SizeKnown = false, want true")
	}
}
