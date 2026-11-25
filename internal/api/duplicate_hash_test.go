package api

import (
	"context"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"riply/internal/config"
	"riply/internal/drive"
	"riply/internal/library"
	"riply/internal/metrics"
	"riply/internal/sse"
)

// The disc hash must win over the name check: a hash match warns even when
// the disc name is generic, while the same generic name with a different
// hash must stay silent. An empty hash falls back to the name check.
func TestDuplicateWarningHashBeatsName(t *testing.T) {
	st, err := library.Open(filepath.Join(t.TempDir(), "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	if err := st.Add(context.Background(), library.Entry{
		Title: "Real Movie", DiscName: "DVD_VIDEO", DiscHash: "hash-a",
		MediaType: "movie", OutputPath: "/m/Real Movie/main.mkv", Source: "rip",
	}); err != nil {
		t.Fatal(err)
	}

	scanner := drive.New()
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DriveDiscovered, Path: "/dev/sr9"})
	scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DiscInserted, Path: "/dev/sr9"})
	sseMgr := sse.New()
	s := New(config.Default(), filepath.Join(t.TempDir(), "config.toml"), scanner, nil, nil, sseMgr, &metrics.Metrics{}, st)

	// expectNext reads one event from an already-subscribed sub.
	expectNext := func(t *testing.T, sub *sse.Subscriber, wantType, contains string) {
		t.Helper()
		select {
		case ev := <-sub.Chan():
			if ev.Type != wantType {
				t.Fatalf("event type = %q, want %q", ev.Type, wantType)
			}
			if contains != "" && !strings.Contains(ev.Message, contains) {
				t.Errorf("message = %q, want it to contain %q", ev.Message, contains)
			}
		case <-time.After(2 * time.Second):
			t.Fatal("no event published")
		}
	}
	// expectSilent asserts nothing arrives within 300ms.
	expectSilent := func(t *testing.T, sub *sse.Subscriber) {
		t.Helper()
		select {
		case ev := <-sub.Chan():
			t.Fatalf("unexpected event %q (%s)", ev.Type, ev.Message)
		case <-time.After(300 * time.Millisecond):
		}
	}

	// Hash match: warns despite the generic name.
	sub1 := sseMgr.Subscribe()
	go s.duplicateWarning(context.Background(), "/dev/sr9", "DVD_VIDEO", "hash-a")
	expectNext(t, sub1, "rip_duplicate", "hash match")

	// Same generic name, different disc: silent.
	sub2 := sseMgr.Subscribe()
	s.duplicateWarning(context.Background(), "/dev/sr9", "DVD_VIDEO", "hash-b")
	expectSilent(t, sub2)

	// No hash available: name-based fallback still warns.
	sub3 := sseMgr.Subscribe()
	go s.duplicateWarning(context.Background(), "/dev/sr9", "dvd_video", "")
	expectNext(t, sub3, "rip_duplicate", "")
}
