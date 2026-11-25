package naming

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"runtime/debug"
	"strconv"
	"strings"
	"sync"
	"time"

	"riply/internal/logger"
	"riply/internal/version"
)

// Result represents a TMDb match (movie or TV).
type Result struct {
	ID       int    `json:"id"`
	Title    string `json:"title"`
	Year     int    `json:"year"`
	Poster   string `json:"poster"`
	Overview string `json:"overview"`
	IsTV     bool   `json:"is_tv,omitempty"`
}

// searchCacheEntry holds a cached TMDb search result set.
type searchCacheEntry struct {
	results   []Result
	expiresAt time.Time
}

const (
	// searchCacheTTL bounds how long a search result set is reused.
	searchCacheTTL = time.Hour
	// searchCacheMaxEntries bounds the in-memory cache size.
	searchCacheMaxEntries = 200
)

// Client handles TMDb API interactions.
type Client struct {
	mu          sync.Mutex
	apiKey      string
	baseURL     string
	client      *http.Client
	lastReq     time.Time
	cooldown    time.Duration
	cacheMu     sync.Mutex
	searchCache map[string]searchCacheEntry
}

// userAgent identifies Riply to TMDb; the Terms of Use forbid obscuring the
// caller's identity, and the Go default ("Go-http-client/1.1") does exactly
// that. Version comes from the version package by default, and is overridden
// by build info when built as a tagged release.
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

// NewClient creates a new TMDb client with rate limiting. The client is always
// returned; an empty apiKey means naming is disabled until SetAPIKey supplies a
// key (Search then short-circuits without any HTTP request).
func NewClient(apiKey string) *Client {
	return &Client{
		apiKey:      apiKey,
		baseURL:     "https://api.themoviedb.org/3",
		client:      &http.Client{Timeout: 10 * time.Second, Transport: userAgentRoundTripper{next: http.DefaultTransport}},
		cooldown:    1 * time.Second,
		searchCache: make(map[string]searchCacheEntry),
	}
}

// SetAPIKey updates the key used for subsequent requests; empty disables naming.
func (c *Client) SetAPIKey(key string) {
	c.mu.Lock()
	c.apiKey = key
	c.mu.Unlock()
}

// APIKey returns the API key under the client mutex. A nil client has no key.
func (c *Client) APIKey() string {
	if c == nil {
		return ""
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.apiKey
}

// Search searches TMDb for movies or TV shows matching the query, honouring
// ctx for both the rate-limit wait and the HTTP request. Callers pass their
// request or base context so server shutdown cancels in-flight searches.
func (c *Client) Search(ctx context.Context, query string) ([]Result, error) {
	if c == nil {
		return nil, errors.New("naming client not configured")
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if c.APIKey() == "" {
		return nil, errors.New("TMDb API key not set")
	}
	key := cacheKey(query)
	if results, ok := c.cachedSearch(key); ok {
		logger.Debug("TMDb search cache hit", "query", key, "count", len(results))
		return results, nil
	}
	if err := c.rateLimit(ctx); err != nil {
		return nil, err
	}
	if results, ok := c.cachedSearch(key); ok {
		logger.Debug("TMDb search cache hit", "query", key, "count", len(results))
		return results, nil
	}

	logger.Debug("TMDb search", "query", query)

	params := url.Values{}
	params.Set("api_key", c.APIKey())
	params.Set("query", query)
	params.Set("include_adult", "false")
	params.Set("page", "1")

	endpoint := fmt.Sprintf("%s/search/multi?%s", c.baseURL, params.Encode())

	// M2: one bounded retry on 429, sleeping whatever Retry-After advertises.
	// Other non-200 responses fail immediately (5xx retry deliberately out of
	// scope); no unbounded retries anywhere.
	var resp *http.Response
	for attempt := 0; ; attempt++ {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
		if err != nil {
			logger.Error("TMDb search failed", "error", err)
			return nil, err
		}

		resp, err = c.client.Do(req)
		if err != nil {
			err = redactURLError(err)
			logger.Error("TMDb search failed", "error", err)
			return nil, err
		}

		if resp.StatusCode == http.StatusTooManyRequests && attempt == 0 {
			wait := c.retryAfter(resp.Header.Get("Retry-After"))
			io.Copy(io.Discard, resp.Body)
			resp.Body.Close()
			logger.Warn("TMDb rate limited; retrying once", "retry_after", wait)
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
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusTooManyRequests {
		body, _ := io.ReadAll(resp.Body)
		logger.Error("TMDb API error", "status", resp.StatusCode, "body", string(body))
		return nil, fmt.Errorf("TMDb rate limited (429): %s", string(body))
	}

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		logger.Error("TMDb API error", "status", resp.StatusCode, "body", string(body))
		return nil, fmt.Errorf("TMDb API error %d: %s", resp.StatusCode, string(body))
	}

	var data struct {
		Results []struct {
			ID           int    `json:"id"`
			Title        string `json:"title"`
			Name         string `json:"name"`
			MediaType    string `json:"media_type"`
			Year         string `json:"release_date"`
			FirstAirYear string `json:"first_air_date"`
			Poster       string `json:"poster_path"`
			Overview     string `json:"overview"`
		} `json:"results"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}

	results := make([]Result, 0, len(data.Results))
	for _, r := range data.Results {
		// Filter: only accept "movie" and "tv" result types.
		// search/multi also returns "person", "game", "collection", etc.
		if r.MediaType != "movie" && r.MediaType != "tv" {
			continue
		}

		year := 0
		if len(r.Year) >= 4 {
			fmt.Sscanf(r.Year, "%d", &year)
		} else if len(r.FirstAirYear) >= 4 {
			fmt.Sscanf(r.FirstAirYear, "%d", &year)
		}

		title := r.Title
		isTV := r.MediaType == "tv"
		if isTV {
			title = r.Name
		}

		poster := ""
		if r.Poster != "" {
			poster = fmt.Sprintf("https://image.tmdb.org/t/p/w200%s", r.Poster)
		}

		results = append(results, Result{
			ID:       r.ID,
			Title:    title,
			Year:     year,
			Poster:   poster,
			Overview: r.Overview,
			IsTV:     isTV,
		})
	}

	logger.Info("TMDb search results", "count", len(results), "query", query)
	if len(results) == 0 {
		logger.Warn("TMDb search: no results", "query", query)
	}
	c.storeSearch(key, results)

	return results, nil
}

// redactURLError strips the request URL from a transport error before it is
// logged or returned: the TMDb endpoint carries api_key as a query parameter,
// and *url.Error formats the full URL into its message. The transport cause is
// kept so errors.Is/errors.As still match (e.g. context.DeadlineExceeded).
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

// rateLimit enforces a minimum interval between API requests. The wait is
// computed under the mutex but slept outside it, and lastReq records the time
// this caller's request actually fires — never a reserved future slot, so a
// concurrent caller can piggyback on the same cooldown window instead of
// stacking behind every other caller's reservation (which serialized waits to
// N×cooldown). Piggybacked requests share one window; at 1 req/s TMDb limits
// that is an acceptable trade against never blocking handlers for seconds.
func (c *Client) rateLimit(ctx context.Context) error {
	c.mu.Lock()
	now := time.Now()
	var wait time.Duration
	if !c.lastReq.IsZero() {
		if d := c.cooldown - now.Sub(c.lastReq); d > 0 {
			wait = d
		}
	}
	c.lastReq = now
	c.mu.Unlock()

	if wait > 0 {
		t := time.NewTimer(wait)
		defer t.Stop()
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-t.C:
		}
	}
	return ctx.Err()
}

// retryAfter parses a Retry-After header value (delta-seconds, or an HTTP-date
// whose distance is clamped to [0, 30s]) into a sleep duration. Missing or
// unparseable values fall back to the cooldown window.
func (c *Client) retryAfter(value string) time.Duration {
	const maxRetryWait = 30 * time.Second
	if value == "" {
		return c.cooldown
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
	return c.cooldown
}

// cacheKey normalizes a query so "MASK", "Mask", and " mask " share one entry.
func cacheKey(query string) string {
	return strings.ToLower(strings.Join(strings.Fields(query), " "))
}

// cachedSearch returns a copy of a live cached result set for key.
func (c *Client) cachedSearch(key string) ([]Result, bool) {
	c.cacheMu.Lock()
	defer c.cacheMu.Unlock()
	entry, ok := c.searchCache[key]
	if !ok {
		return nil, false
	}
	if time.Now().After(entry.expiresAt) {
		delete(c.searchCache, key)
		return nil, false
	}
	results := make([]Result, len(entry.results))
	copy(results, entry.results)
	return results, true
}

// storeSearch caches results for key, evicting expired then oldest entries.
func (c *Client) storeSearch(key string, results []Result) {
	c.cacheMu.Lock()
	defer c.cacheMu.Unlock()
	if len(c.searchCache) >= searchCacheMaxEntries {
		now := time.Now()
		for k, e := range c.searchCache {
			if now.After(e.expiresAt) {
				delete(c.searchCache, k)
			}
		}
		for k := range c.searchCache {
			if len(c.searchCache) < searchCacheMaxEntries {
				break
			}
			delete(c.searchCache, k)
		}
	}
	c.searchCache[key] = searchCacheEntry{
		results:   append([]Result(nil), results...),
		expiresAt: time.Now().Add(searchCacheTTL),
	}
}

// episodeMarker matches episode indicators in disc titles.
var episodeMarker = regexp.MustCompile(`\s*[-–—]\s*(?:S\d+E\d+|Ep\s*\d+|Episode\s*\d+)\s*[-–—]?\s*.*`)

// ParseTitle extracts a clean title from a MakeMKV disc title string.
// It strips episode markers and trailing episode titles.
// Examples:
//
//	"Breaking Bad - S01E01 - Pilot" → "Breaking Bad"
//	"Inception" → "Inception"
//	"Avatar: The Way of Water" → "Avatar: The Way of Water"
//	"KLOVN - Ep 1" → "KLOVN"
func ParseTitle(raw string) string {
	if raw == "" {
		return ""
	}

	// Strip episode markers (e.g., " - S01E01 - ", " - Ep 1 - ")
	cleaned := episodeMarker.ReplaceAllString(raw, "")

	// Strip trailing episode title if present (split on " - ")
	parts := strings.SplitN(strings.TrimSpace(cleaned), " - ", 2)
	return strings.TrimSpace(parts[0])
}

var (
	seasonEpisodeRe  = regexp.MustCompile(`(?i)\bS(\d{1,2})\s*E(\d{1,3})\b`)
	seasonOnlyRe     = regexp.MustCompile(`(?i)\bSeason\s*(\d{1,2})\b|\bS(\d{1,2})\b`)
	episodeOnlyRe    = regexp.MustCompile(`(?i)\b(?:Ep|Episode)\s*(\d{1,3})\b`)
	trailingSeasonRe = regexp.MustCompile(`(?i)^(.*?)\s*[-–—:]?\s*(?:Season\s*\d{1,2}|S\d{1,2})$`)
)

// ParseSeasonEpisode extracts a season and/or episode marker from a disc title
// or filename. Missing values are 0. "KLOVN S01" → (1,0);
// "KLOVN - S02E03 - Pilot" → (2,3); "KLOVN - Ep 4" → (0,4).
func ParseSeasonEpisode(raw string) (season, episode int) {
	if raw == "" {
		return 0, 0
	}
	if m := seasonEpisodeRe.FindStringSubmatch(raw); m != nil {
		season, _ = strconv.Atoi(m[1])
		episode, _ = strconv.Atoi(m[2])
		return season, episode
	}
	if m := seasonOnlyRe.FindStringSubmatch(raw); m != nil {
		if m[1] != "" {
			season, _ = strconv.Atoi(m[1])
		} else if m[2] != "" {
			season, _ = strconv.Atoi(m[2])
		}
		return season, 0
	}
	if m := episodeOnlyRe.FindStringSubmatch(raw); m != nil {
		episode, _ = strconv.Atoi(m[1])
	}
	return 0, episode
}

// ParseSeriesTitle is ParseTitle with a trailing season marker removed:
// "KLOVN S01" → "KLOVN"; "Breaking Bad - S01E01 - Pilot" → "Breaking Bad".
func ParseSeriesTitle(raw string) string {
	t := strings.TrimSpace(ParseTitle(raw))
	if m := trailingSeasonRe.FindStringSubmatch(t); m != nil {
		t = strings.TrimSpace(m[1])
	}
	if t == "" {
		return strings.TrimSpace(ParseTitle(raw))
	}
	return t
}
