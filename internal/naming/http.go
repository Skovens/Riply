package naming

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"runtime/debug"
	"strconv"
	"sync"
	"time"

	"riply/internal/logger"
	"riply/internal/version"
)

// userAgent identifies Riply to metadata providers; the TMDb Terms of Use
// forbid obscuring the caller's identity, and the Go default
// ("Go-http-client/1.1") does exactly that. Version comes from the version
// package by default, and is overridden by build info when built as a tagged
// release.
func userAgent() string {
	v := version.Version
	if bi, ok := debug.ReadBuildInfo(); ok && bi.Main.Version != "" && bi.Main.Version != "(devel)" {
		v = bi.Main.Version
	}
	return "riply/" + v + " (+https://github.com/Skovens/Riply)"
}

// userAgentRoundTripper attaches that identity to every outgoing request
// without touching the call sites.
type userAgentRoundTripper struct{ next http.RoundTripper }

func (rt userAgentRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
	r := req.Clone(req.Context())
	r.Header.Set("User-Agent", userAgent())
	return rt.next.RoundTrip(r)
}

// rateLimiter spaces successive requests by cooldown — a per-provider
// constant hard-coded to each provider's published limits, never a
// user-facing config field. The wait is computed under the mutex but slept
// outside it, and lastReq records the time this caller's request actually
// fires — never a reserved future slot, so a concurrent caller can piggyback
// on the same cooldown window instead of stacking behind every other
// caller's reservation (which serialized waits to N×cooldown). Piggybacked
// requests share one window; at 1 req/s TMDb limits that is an acceptable
// trade against never blocking handlers for seconds.
type rateLimiter struct {
	mu       sync.Mutex
	lastReq  time.Time
	cooldown time.Duration
}

func (r *rateLimiter) wait(ctx context.Context) error {
	r.mu.Lock()
	now := time.Now()
	var d time.Duration
	if !r.lastReq.IsZero() {
		if wait := r.cooldown - now.Sub(r.lastReq); wait > 0 {
			d = wait
		}
	}
	r.lastReq = now
	r.mu.Unlock()

	if d > 0 {
		t := time.NewTimer(d)
		defer t.Stop()
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-t.C:
		}
	}
	return ctx.Err()
}

// retryAfter parses a Retry-After header value (delta-seconds, or an
// HTTP-date whose distance is clamped to [0, 30s]) into a sleep duration.
// Missing or unparseable values fall back to the cooldown window.
func (r *rateLimiter) retryAfter(value string) time.Duration {
	const maxRetryWait = 30 * time.Second
	if value == "" {
		return r.cooldown
	}
	if secs, err := strconv.Atoi(value); err == nil {
		if secs < 0 {
			secs = 0
		}
		if d := time.Duration(secs) * time.Second; d < maxRetryWait {
			return d
		}
		return maxRetryWait
	}
	if t, err := http.ParseTime(value); err == nil {
		d := time.Until(t)
		if d < 0 {
			return 0
		}
		if d > maxRetryWait {
			return maxRetryWait
		}
		return d
	}
	return r.cooldown
}

// httpProvider carries the state shared by HTTP-backed providers: the
// mutable API key, the HTTP client with the identity round-tripper, and the
// per-provider rate limiter. Concrete providers embed it and implement the
// endpoint + response mapping.
type httpProvider struct {
	mu      sync.Mutex
	apiKey  string
	baseURL string
	client  *http.Client
	rl      rateLimiter
}

func (p *httpProvider) setAPIKey(key string) {
	p.mu.Lock()
	p.apiKey = key
	p.mu.Unlock()
}

func (p *httpProvider) hasKey() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.apiKey != ""
}

func (p *httpProvider) key() string {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.apiKey
}

// get performs one provider request: rate-limited, with one bounded retry on
// 429 sleeping whatever Retry-After advertises (M2). Other non-200 responses
// fail immediately (5xx retry deliberately out of scope); no unbounded
// retries anywhere. Transport errors are redacted — provider endpoints carry
// api_key as a query parameter, and *url.Error formats the full URL into its
// message.
func (p *httpProvider) get(ctx context.Context, name, endpoint string) (*http.Response, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if err := p.rl.wait(ctx); err != nil {
		return nil, err
	}

	var resp *http.Response
	for attempt := 0; ; attempt++ {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
		if err != nil {
			logger.Error(name+" search failed", "error", err)
			return nil, err
		}

		resp, err = p.client.Do(req)
		if err != nil {
			err = redactURLError(err)
			logger.Error(name+" search failed", "error", err)
			return nil, err
		}

		if resp.StatusCode == http.StatusTooManyRequests && attempt == 0 {
			wait := p.rl.retryAfter(resp.Header.Get("Retry-After"))
			io.Copy(io.Discard, resp.Body)
			resp.Body.Close()
			logger.Warn(name+" rate limited; retrying once", "retry_after", wait)
			t := time.NewTimer(wait)
			select {
			case <-ctx.Done():
				t.Stop()
				return nil, ctx.Err()
			case <-t.C:
			}
			continue
		}
		break
	}
	return resp, nil
}

// errorForBody turns a non-200 response into a redacted error.
func (p *httpProvider) errorForBody(name string, resp *http.Response) error {
	body, _ := io.ReadAll(resp.Body)
	logger.Error(name+" API error", "status", resp.StatusCode, "body", string(body))
	if resp.StatusCode == http.StatusTooManyRequests {
		return fmt.Errorf("%s rate limited (429): %s", name, string(body))
	}
	return fmt.Errorf("%s API error %d: %s", name, resp.StatusCode, string(body))
}

// redactURLError strips the request URL from a transport error before it is
// logged or returned: provider endpoints carry api_key as a query parameter,
// and *url.Error formats the full URL into its message. The transport cause
// is kept so errors.Is/errors.As still match (e.g. context.DeadlineExceeded).
func redactURLError(err error) error {
	var ue *url.Error
	if !errors.As(err, &ue) {
		return err
	}
	clean := &url.Error{Op: ue.Op, URL: "[redacted]", Err: ue.Err}
	if u, pErr := url.Parse(ue.URL); pErr == nil {
		u.RawQuery = ""
		u.Opaque = ""
		u.Fragment = ""
		clean.URL = u.String()
	}
	return clean
}

// yearPrefix parses the leading year of values like "2010", "2010-07-16",
// or "2010–2013" ("2010–2013" → 2010). 0 when unparsable.
func yearPrefix(s string) int {
	runes := []rune(s)
	i := 0
	for i < len(runes) && runes[i] >= '0' && runes[i] <= '9' {
		i++
	}
	if i < 4 {
		return 0
	}
	year, err := strconv.Atoi(string(runes[:i]))
	if err != nil {
		return 0
	}
	return year
}
