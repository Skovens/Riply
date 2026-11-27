package naming

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// M2: a 429 with Retry-After: 1 → client waits ~1s, retries once, succeeds.
func TestSearchRetriesOnceOn429(t *testing.T) {
	var calls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if calls.Add(1) == 1 {
			w.Header().Set("Retry-After", "1")
			w.WriteHeader(http.StatusTooManyRequests)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"results":[{"id":1,"title":"Inception","release_date":"2010-07-16"}]}`))
	}))
	defer srv.Close()

	p := newTMDBProvider("k")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	start := time.Now()
	results, err := p.Search(context.Background(), "Inception", Movie)
	if err != nil {
		t.Fatalf("Search after 429 retry = %v", err)
	}
	if calls.Load() != 2 {
		t.Errorf("calls = %d, want 2 (one retry)", calls.Load())
	}
	if len(results) != 1 || results[0].Title != "Inception" {
		t.Errorf("results = %+v", results)
	}
	if elapsed := time.Since(start); elapsed < 900*time.Millisecond {
		t.Errorf("returned after %v; expected to honour Retry-After: 1 (~1s)", elapsed)
	}
}

// M2: second 429 after the single retry → error mentioning rate limit, never
// a third call (bounded retries).
func TestSearch429TwiceGivesUp(t *testing.T) {
	var calls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		w.Header().Set("Retry-After", "0")
		w.WriteHeader(http.StatusTooManyRequests)
	}))
	defer srv.Close()

	p := newTMDBProvider("k")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	_, err := p.Search(context.Background(), "Inception", Movie)
	if err == nil {
		t.Fatal("expected error on persistent 429")
	}
	if !strings.Contains(err.Error(), "429") || !strings.Contains(err.Error(), "rate limit") {
		t.Errorf("err = %v; want it to mention rate limit/429", err)
	}
	if calls.Load() != 2 {
		t.Errorf("calls = %d, want 2 (bounded: original + one retry)", calls.Load())
	}
}

// M2: non-429 errors must not be retried (5xx retry deliberately out-of-scope).
func TestSearchNoRetryOn500(t *testing.T) {
	var calls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer srv.Close()

	p := newTMDBProvider("k")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	_, err := p.Search(context.Background(), "Inception", Movie)
	if err == nil {
		t.Fatal("expected error on 500")
	}
	if calls.Load() != 1 {
		t.Errorf("calls = %d, want 1 (no retry on 5xx)", calls.Load())
	}
}

// retryAfter parsing: delta-seconds, HTTP-date, missing/garbage → fallback.
func TestRetryAfterParsing(t *testing.T) {
	rl := &rateLimiter{cooldown: 1 * time.Second}

	if got := rl.retryAfter("2"); got != 2*time.Second {
		t.Errorf("retryAfter(\"2\") = %v", got)
	}
	if got := rl.retryAfter("3600"); got != 30*time.Second {
		t.Errorf("retryAfter(\"3600\") = %v, want clamp to 30s", got)
	}
	if got := rl.retryAfter(""); got != time.Second {
		t.Errorf("retryAfter(\"\") = %v, want cooldown", got)
	}
	if got := rl.retryAfter("garbage"); got != time.Second {
		t.Errorf("retryAfter(garbage) = %v, want cooldown", got)
	}
	future := time.Now().Add(5 * time.Second).UTC().Format(http.TimeFormat)
	if got := rl.retryAfter(future); got < 2*time.Second || got > 5*time.Second {
		t.Errorf("retryAfter(HTTP-date) = %v, want ~4-5s", got)
	}
}

// L2: TMDb Terms forbid obscuring the caller's identity — every request must
// carry the riply User-Agent (never the Go default).
func TestSearchSetsRiplyUserAgent(t *testing.T) {
	var gotUA string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotUA = r.Header.Get("User-Agent")
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"results":[{"id":1,"title":"X","release_date":"2020-01-01","poster_path":"/p.jpg"}]}`))
	}))
	defer srv.Close()

	p := newTMDBProvider("key")
	p.rl.cooldown = 0
	p.baseURL = srv.URL
	if _, err := p.Search(context.Background(), "x", Movie); err != nil {
		t.Fatalf("Search = %v", err)
	}
	if !strings.HasPrefix(gotUA, "riply/") || !strings.Contains(gotUA, "+https://github.com/Skovens/Riply") {
		t.Errorf("User-Agent = %q, want riply/<version> (+https://github.com/Skovens/Riply)", gotUA)
	}
	if gotUA == "Go-http-client/1.1" {
		t.Error("default Go User-Agent sent")
	}
}
