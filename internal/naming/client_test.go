package naming

import (
	"context"
	"sync"
	"testing"
	"time"
)

// New must always return a usable chain, even with no keys: a key saved
// later via Settings enables naming without a restart.
func TestNewAlwaysNonNil(t *testing.T) {
	c := New(nil, nil, nil)
	if c == nil {
		t.Fatal("New(nil, nil, nil) returned nil")
	}
	if c.Enabled() {
		t.Error("keyless chain reports Enabled")
	}
}

// Providers without keys are skipped at search time; with a key the
// provider is enabled and SetAPIKey toggles the state live.
func TestSetAPIKeyTogglesState(t *testing.T) {
	c := New([]ConfiguredProvider{{Name: "tmdb", APIKey: ""}}, nil, nil)
	if c.Enabled() {
		t.Fatal("empty-key chain reports Enabled")
	}
	if _, err := c.SearchAuto(context.Background(), "Inception"); err == nil {
		t.Fatal("SearchAuto succeeded without any key")
	}
	c.SetAPIKey("tmdb", "k")
	if !c.Enabled() {
		t.Fatal("chain still disabled after SetAPIKey")
	}
	c.SetAPIKey("tmdb", "")
	if c.Enabled() {
		t.Fatal("chain still enabled after key cleared")
	}
}

// SetOrders replaces the try-orders live; empty lists leave them unchanged.
func TestSetOrders(t *testing.T) {
	c := New(
		[]ConfiguredProvider{{Name: "tmdb", APIKey: "k"}, {Name: "omdb", APIKey: "k"}},
		[]string{"tmdb"}, []string{"tmdb"})
	if got := c.orderFor(Movie); got[0].Name() != "tmdb" {
		t.Fatalf("movie order head = %s, want tmdb", got[0].Name())
	}
	c.SetOrders([]string{"omdb"}, []string{"omdb", "tmdb"})
	if got := c.orderFor(Movie); got[0].Name() != "omdb" {
		t.Errorf("movie order head after SetOrders = %s, want omdb", got[0].Name())
	}
	if got := c.orderFor(TV); len(got) != 2 || got[0].Name() != "omdb" || got[1].Name() != "tmdb" {
		t.Errorf("tv order after SetOrders = %v", names(got))
	}
	// Empty lists leave the previous orders in place.
	c.SetOrders(nil, nil)
	if got := c.orderFor(Movie); got[0].Name() != "omdb" {
		t.Errorf("movie order changed by empty SetOrders: %v", names(got))
	}
}

func names(ps []Provider) []string {
	out := make([]string, 0, len(ps))
	for _, p := range ps {
		out = append(out, p.Name())
	}
	return out
}

// Concurrent callers must never wait a serialized multiple of the cooldown.
// With lastReq recorded as the actual request time (not a reserved future
// slot), each caller independently waits at most ~1×cooldown; with the old
// reservation, caller N waited ~(N-1)×cooldown.
func TestRateLimitConcurrentWaitBounded(t *testing.T) {
	const cooldown = 150 * time.Millisecond
	p := newTMDBProvider("k")
	p.rl.cooldown = cooldown
	p.rl.wait(context.Background()) // establish a fresh slot

	const n = 3
	var wg sync.WaitGroup
	waited := make([]time.Duration, n)
	for i := range waited {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			start := time.Now()
			p.rl.wait(context.Background())
			waited[i] = time.Since(start)
		}(i)
	}
	wg.Wait()

	for i, d := range waited {
		if d > cooldown+60*time.Millisecond {
			t.Errorf("rateLimit call %d waited %v; want <= ~1x cooldown (caller waited behind a reserved future slot)", i, d)
		}
	}
}

// The rate limiter must not hold its mutex while sleeping: a second
// goroutine reading the key must return in milliseconds while the first is
// mid-cooldown.
func TestRateLimitDoesNotHoldLockWhileSleeping(t *testing.T) {
	const cooldown = 200 * time.Millisecond
	p := newTMDBProvider("k")
	p.rl.cooldown = cooldown
	p.rl.wait(context.Background()) // establish a fresh slot

	go p.rl.wait(context.Background()) // sleeps ~cooldown, must not hold p.mu

	done := make(chan struct{})
	go func() {
		p.key()
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(cooldown / 2):
		t.Fatal("key read blocked behind rateLimiter's sleep; mutex held across sleep")
	}
}
