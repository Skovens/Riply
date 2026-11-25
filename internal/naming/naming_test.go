package naming

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

func TestParseTitle(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"MASK", "MASK"},
		{"The Mask - S01E01 - Pilot", "The Mask"},
		{"Breaking Bad - S01E01 - Pilot", "Breaking Bad"},
		{"Avatar: The Way of Water", "Avatar: The Way of Water"},
		{"KLOVN - Ep 1", "KLOVN"},
		{"", ""},
		{"Unknown", "Unknown"},
	}
	for _, tt := range tests {
		got := ParseTitle(tt.input)
		if got != tt.want {
			t.Errorf("ParseTitle(%q) = %q, want %q", tt.input, got, tt.want)
		}
	}
}

type failingTransport struct{ err error }

func (f failingTransport) RoundTrip(*http.Request) (*http.Response, error) { return nil, f.err }

// H1: the TMDb endpoint embeds api_key in the query; a transport failure's
// *url.Error must not carry it into logs or API responses, and must still
// match the transport cause via errors.Is.
func TestSearchTransportErrorRedactsAPIKey(t *testing.T) {
	sentinel := errors.New("dial tcp: connect refused")
	c := NewClient("sup3rsecretkey")
	c.cooldown = 0
	c.client.Transport = failingTransport{err: sentinel}

	_, err := c.Search(context.Background(), "Inception")
	if err == nil {
		t.Fatal("expected error from failing transport")
	}
	if strings.Contains(err.Error(), "api_key") || strings.Contains(err.Error(), "sup3rsecretkey") {
		t.Errorf("error leaks API key: %v", err)
	}
	if !errors.Is(err, sentinel) {
		t.Errorf("redacted error no longer matches transport cause: %v (%T)", err, err)
	}
	var ue *url.Error
	if !errors.As(err, &ue) {
		t.Fatalf("expected *url.Error, got %T: %v", err, err)
	}
	if strings.Contains(ue.URL, "?") {
		t.Errorf("url.Error URL retains query: %s", ue.URL)
	}
}

// Non-transport errors pass through redactURLError unchanged.
func TestRedactURLErrorPassesThroughOthers(t *testing.T) {
	other := errors.New("plain")
	if got := redactURLError(other); got != other {
		t.Errorf("redactURLError(non-url.Error) = %v, want same error", got)
	}
}

// M1: a slow TMDb response must be cancellable through ctx — shutdown or a
// gone client cancels the in-flight search instead of blocking for the full
// client timeout.
func TestSearchHonoursContextCancel(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
		case <-time.After(5 * time.Second):
		}
	}))
	defer srv.Close()

	c := NewClient("k")
	c.cooldown = 0
	c.baseURL = srv.URL

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(100 * time.Millisecond)
		cancel()
	}()

	start := time.Now()
	_, err := c.Search(ctx, "Inception")
	if !errors.Is(err, context.Canceled) {
		t.Errorf("Search with cancelled ctx = %v, want context.Canceled", err)
	}
	if elapsed := time.Since(start); elapsed > 2*time.Second {
		t.Errorf("Search blocked %v after cancel; want prompt cancellation", elapsed)
	}
}

// M1: an already-cancelled context short-circuits before any HTTP.
func TestSearchContextPreCancelled(t *testing.T) {
	calls := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
	}))
	defer srv.Close()

	c := NewClient("k")
	c.cooldown = 0
	c.baseURL = srv.URL

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := c.Search(ctx, "Inception"); !errors.Is(err, context.Canceled) {
		t.Errorf("pre-cancelled Search = %v, want context.Canceled", err)
	}
	if calls != 0 {
		t.Errorf("pre-cancelled Search made %d HTTP calls, want 0", calls)
	}
}

// M1: the rate-limit wait must also honour ctx — a cancelled caller is
// released early, it does not sleep the full cooldown.
func TestRateLimitCancelDuringWait(t *testing.T) {
	c := NewClient("k")
	c.cooldown = 10 * time.Second
	c.rateLimit(context.Background()) // occupy the slot, next call must wait ~10s

	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	start := time.Now()
	err := c.rateLimit(ctx)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Errorf("rateLimit with expiring ctx = %v, want DeadlineExceeded", err)
	}
	if elapsed := time.Since(start); elapsed > 2*time.Second {
		t.Errorf("rateLimit waited %v; want prompt release on ctx done", elapsed)
	}
}
