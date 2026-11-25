package api

import (
	"bufio"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"riply/internal/sse"
)

// L10: an idle SSE stream must receive ": keepalive" comment frames so
// intermediates don't reap it, and they must keep coming (ticker, not a
// one-shot). EventSource ignores comment lines, so this is wire-level only.
func TestSSEKeepaliveFrames(t *testing.T) {
	orig := sseKeepaliveInterval
	sseKeepaliveInterval = 15 * time.Millisecond
	t.Cleanup(func() { sseKeepaliveInterval = orig })

	s, _ := newRipServer(t, &instantRipper{})
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	ts := httptest.NewServer(mux)
	defer ts.Close()

	resp, err := ts.Client().Get(ts.URL + "/events")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if ct := resp.Header.Get("Content-Type"); ct != "text/event-stream" {
		t.Fatalf("Content-Type = %q, want text/event-stream", ct)
	}

	br := bufio.NewReader(resp.Body)
	seen := 0
	deadline := time.Now().Add(3 * time.Second)
	for seen < 2 && time.Now().Before(deadline) {
		line, err := br.ReadString('\n')
		if err != nil && err != io.EOF {
			t.Fatalf("read from stream: %v", err)
		}
		if strings.HasPrefix(line, ": keepalive") {
			seen++
		}
	}
	if seen < 2 {
		t.Errorf("keepalive comment frames seen = %d, want >= 2 within 3s", seen)
	}
}

// A published event must still arrive interleaved with keepalives.
func TestSSEEventAndKeepaliveCoexist(t *testing.T) {
	orig := sseKeepaliveInterval
	sseKeepaliveInterval = 15 * time.Millisecond
	t.Cleanup(func() { sseKeepaliveInterval = orig })

	s, _ := newRipServer(t, &instantRipper{})
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	ts := httptest.NewServer(mux)
	defer ts.Close()

	resp, err := ts.Client().Get(ts.URL + "/events")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	br := bufio.NewReader(resp.Body)

	// Publish after a small delay so at least one keepalive precedes it.
	go func() {
		time.Sleep(60 * time.Millisecond)
		s.sseMgr.Publish(sse.Event{Type: "shutdown", Message: "Server shutting down"})
	}()

	gotEvent, gotKeep := false, false
	deadline := time.Now().Add(3 * time.Second)
	for (!gotEvent || !gotKeep) && time.Now().Before(deadline) {
		line, err := br.ReadString('\n')
		if err != nil {
			break
		}
		switch {
		case strings.HasPrefix(line, ": keepalive"):
			gotKeep = true
		case strings.HasPrefix(line, "event: shutdown"):
			gotEvent = true
		}
	}
	if !gotKeep {
		t.Error("no keepalive frame observed")
	}
	if !gotEvent {
		t.Error("published shutdown event did not arrive")
	}
}

// A rip_progress SSE event must carry rip_phase in its progress payload so
// the frontend can distinguish pre-rip scanning from actual file writing.
func TestSSEProgressCarriesRipPhase(t *testing.T) {
	orig := sseKeepaliveInterval
	sseKeepaliveInterval = 15 * time.Millisecond
	t.Cleanup(func() { sseKeepaliveInterval = orig })

	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	ts := httptest.NewServer(mux)
	defer ts.Close()

	resp, err := ts.Client().Get(ts.URL + "/events")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	br := bufio.NewReader(resp.Body)

	go func() {
		time.Sleep(60 * time.Millisecond)
		s.sseMgr.Publish(sse.Event{
			Type:    "rip_progress",
			Drive:   "/dev/sr0",
			Message: "Ripping: 42%",
			Progress: &sse.Progress{
				Percent:  42,
				RipPhase: "saving",
			},
		})
	}()

	gotLine := false
	deadline := time.Now().Add(3 * time.Second)
	for !gotLine && time.Now().Before(deadline) {
		line, err := br.ReadString('\n')
		if err != nil {
			break
		}
		if strings.Contains(line, `"rip_phase":"saving"`) {
			gotLine = true
		}
	}
	if !gotLine {
		t.Error("SSE rip_progress event did not carry rip_phase in progress payload")
	}
}
