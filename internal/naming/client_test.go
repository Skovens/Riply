package naming

import (
	"context"
	"sync"
	"testing"
	"time"
)

// NewClient must never return nil, even with an empty key: the client exists so
// a later SetAPIKey enables naming without a restart.
func TestNewClientAlwaysNonNil(t *testing.T) {
	if c := NewClient(""); c == nil {
		t.Fatal("NewClient(\"\") returned nil")
	} else if c.APIKey() != "" {
		t.Errorf("empty-key client reports key %q", c.APIKey())
	}
	if c := NewClient("abc"); c == nil || c.APIKey() != "abc" {
		t.Fatalf("NewClient(\"abc\") = %v with key %q", c, c.APIKey())
	}
}

// Search must short-circuit on an empty/nil client without any HTTP request.
func TestSearchDisabledWithoutKey(t *testing.T) {
	c := NewClient("")
	if got, err := c.Search(context.Background(), "Inception"); err == nil || got != nil {
		t.Errorf("empty-key Search = (%v, %v); want error", got, err)
	}
	var nilClient *Client
	if got, err := nilClient.Search(context.Background(), "Inception"); err == nil || got != nil {
		t.Errorf("nil-client Search = (%v, %v); want error", got, err)
	}
}

// SetAPIKey toggles the enabled state live.
func TestSetAPIKeyTogglesState(t *testing.T) {
	c := NewClient("")
	if _, err := c.Search(context.Background(), "Inception"); err == nil {
		t.Fatal("Search succeeded before SetAPIKey")
	}
	c.SetAPIKey("k")
	if c.APIKey() != "k" {
		t.Fatalf("APIKey after SetAPIKey = %q", c.APIKey())
	}
	c.SetAPIKey("")
	if _, err := c.Search(context.Background(), "Inception"); err == nil {
		t.Fatal("Search succeeded after key cleared")
	}
}

// Concurrent callers must never wait a serialized multiple of the cooldown.
// With lastReq recorded as the actual request time (not a reserved future
// slot), each caller independently waits at most ~1×cooldown; with the old
// reservation, caller N waited ~(N-1)×cooldown.
func TestRateLimitConcurrentWaitBounded(t *testing.T) {
	const cooldown = 150 * time.Millisecond
	c := NewClient("k")
	c.cooldown = cooldown
	c.rateLimit(context.Background()) // establish a fresh slot

	const n = 3
	var wg sync.WaitGroup
	waited := make([]time.Duration, n)
	for i := range waited {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			start := time.Now()
			c.rateLimit(context.Background())
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

// rateLimit must not hold its mutex while sleeping. With the old held-lock
// sleep, any concurrent APIKey()/SetAPIKey() call blocked behind the sleeper;
// here a second goroutine's APIKey() must return in milliseconds while the
// first is mid-cooldown. (Callers legitimately still serialize on the cooldown
// itself — that is rate limiting; only the lock-hold is the bug.)
func TestRateLimitDoesNotHoldLockWhileSleeping(t *testing.T) {
	const cooldown = 200 * time.Millisecond
	c := NewClient("k")
	c.cooldown = cooldown
	c.rateLimit(context.Background()) // establish a fresh slot

	go c.rateLimit(context.Background()) // sleeps ~cooldown, must not hold c.mu

	done := make(chan struct{})
	go func() {
		c.APIKey()
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(cooldown / 2):
		t.Fatal("APIKey blocked behind rateLimit's sleep; mutex held across sleep")
	}
}
