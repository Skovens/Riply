package naming

import (
	"context"
	"errors"
	"strings"
	"sync"
	"time"

	"riply/internal/logger"
)

// searchCacheEntry holds a cached search result set.
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

// Chain fans out over the registered providers in per-media-type try-orders.
// It implements Provider: Search runs exactly the given type's order
// (manual search toggle, post-rip fallback where classification exists).
// SearchAuto is the disc-insert path: movie order first, TV order
// immediately on shortfall — the naming goroutine never waits for the rip.
type Chain struct {
	providers   []Provider
	moviesOrder []string
	tvOrder     []string

	cacheMu     sync.Mutex
	searchCache map[string]searchCacheEntry
}

// New builds the provider chain. Construction uses plain parameters
// (ConfiguredProvider) so internal/naming never imports internal/config.
// The moviesOrder/tvOrder lists control priority: listed providers come
// first in list order, any remaining registered provider is appended in
// registration order so a configured key is never silently unused.
func New(providers []ConfiguredProvider, moviesOrder, tvOrder []string) *Chain {
	ps := make([]Provider, 0, len(providers))
	for _, cp := range providers {
		switch cp.Name {
		case "tmdb":
			ps = append(ps, newTMDBProvider(cp.APIKey))
		case "omdb":
			ps = append(ps, newOMDBProvider(cp.APIKey))
		default:
			logger.Warn("Unknown metadata provider; ignoring", "provider", cp.Name)
		}
	}
	return &Chain{
		providers:   ps,
		moviesOrder: moviesOrder,
		tvOrder:     tvOrder,
		searchCache: make(map[string]searchCacheEntry),
	}
}

func (c *Chain) Name() string { return "chain" }

// Enabled reports whether any registered provider has a key.
func (c *Chain) Enabled() bool {
	if c == nil {
		return false
	}
	for _, p := range c.providers {
		if p.Enabled() {
			return true
		}
	}
	return false
}

// SetOrders replaces the per-media-type try-orders live (Settings
// PUT /api/config). Empty lists leave the current order unchanged.
func (c *Chain) SetOrders(movies, tv []string) {
	if len(movies) > 0 {
		c.moviesOrder = movies
	}
	if len(tv) > 0 {
		c.tvOrder = tv
	}
}

// SetAPIKey updates one provider's key live (Settings PUT /api/config) and
// invalidates the search cache: cached results were produced under the old
// configuration.
func (c *Chain) SetAPIKey(name, key string) {
	for _, p := range c.providers {
		if p.Name() == name {
			if ks, ok := p.(keySetter); ok {
				ks.SetAPIKey(key)
				c.clearCache()
				logger.Info("Metadata provider key updated", "provider", name, "enabled", key != "")
			}
			return
		}
	}
	logger.Warn("SetAPIKey for unknown provider", "provider", name)
}

// orderFor resolves the effective try-order for mediaType: listed providers
// (in list order) followed by any remaining registered providers in
// registration order. Disabled providers are skipped later, at search time,
// so a key added via Settings puts its provider back into play without a
// restart.
func (c *Chain) orderFor(mediaType MediaType) []Provider {
	names := c.moviesOrder
	if mediaType == TV {
		names = c.tvOrder
	}
	out := make([]Provider, 0, len(c.providers))
	used := make(map[string]bool, len(c.providers))
	for _, name := range names {
		for _, p := range c.providers {
			if p.Name() == name && !used[name] {
				out = append(out, p)
				used[name] = true
				break
			}
		}
	}
	for _, p := range c.providers {
		if !used[p.Name()] {
			out = append(out, p)
		}
	}
	return out
}

func anyEnabled(providers []Provider) bool {
	for _, p := range providers {
		if p.Enabled() {
			return true
		}
	}
	return false
}

// cacheKey normalizes a query so "MASK", "Mask", and " mask " share one
// entry, namespaced by media type so a movie-chain miss can never serve a
// stale TV-chain result (or vice versa).
func cacheKey(mediaType, query string) string {
	return mediaType + "|" + strings.ToLower(strings.Join(strings.Fields(query), " "))
}

// cachedSearch returns a copy of a live cached result set for key.
func (c *Chain) cachedSearch(key string) ([]Result, bool) {
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
// Only non-empty sets are cached: an empty set is "no match now" and may
// become matchable as soon as a key is added or a provider recovers.
func (c *Chain) storeSearch(key string, results []Result) {
	if len(results) == 0 {
		return
	}
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

func (c *Chain) clearCache() {
	c.cacheMu.Lock()
	c.searchCache = make(map[string]searchCacheEntry)
	c.cacheMu.Unlock()
}

// searchOrder walks the order skipping disabled providers. Provider error →
// warn and continue; empty result set → continue; first non-empty result
// set wins (cached); if every enabled provider errored, the last error is
// returned.
func (c *Chain) searchOrder(ctx context.Context, query string, mediaType MediaType, order []Provider, key string) ([]Result, error) {
	var lastErr error
	for _, p := range order {
		if !p.Enabled() {
			continue
		}
		results, err := p.Search(ctx, query, mediaType)
		if err != nil {
			if ctx.Err() != nil {
				return nil, err
			}
			lastErr = err
			logger.Warn("Metadata provider search failed", "provider", p.Name(), "error", err)
			continue
		}
		if len(results) == 0 {
			continue
		}
		c.storeSearch(key, results)
		return results, nil
	}
	if lastErr != nil {
		return nil, lastErr
	}
	return nil, nil
}

// Search runs the given media type's order with that type's filter.
func (c *Chain) Search(ctx context.Context, query string, mediaType MediaType) ([]Result, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	key := cacheKey(string(mediaType), query)
	if results, ok := c.cachedSearch(key); ok {
		logger.Debug("Search cache hit", "type", string(mediaType), "query", query, "count", len(results))
		return results, nil
	}
	order := c.orderFor(mediaType)
	if !anyEnabled(order) {
		return nil, errors.New("no metadata provider enabled")
	}
	return c.searchOrder(ctx, query, mediaType, order, key)
}

// SearchAuto is the insert-time search: movie pass first; when the movie
// side comes up short (every enabled provider errored or returned empty),
// the TV pass runs immediately in the same goroutine — never gated on rip
// state. Errors from the movie pass never abort the TV pass.
func (c *Chain) SearchAuto(ctx context.Context, query string) ([]Result, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	var lastErr error

	movieKey := cacheKey(string(Movie), query)
	if results, ok := c.cachedSearch(movieKey); ok {
		return results, nil
	}
	movieOrder := c.orderFor(Movie)
	if anyEnabled(movieOrder) {
		results, err := c.searchOrder(ctx, query, Movie, movieOrder, movieKey)
		if ctx.Err() != nil {
			return nil, err
		}
		if err != nil {
			lastErr = err
		}
		if len(results) > 0 {
			return results, nil
		}
	}

	// Movie side exhausted → TV side, immediately.
	tvKey := cacheKey(string(TV), query)
	if results, ok := c.cachedSearch(tvKey); ok {
		return results, nil
	}
	tvOrder := c.orderFor(TV)
	if !anyEnabled(tvOrder) {
		if lastErr != nil {
			return nil, lastErr
		}
		return nil, errors.New("no metadata provider enabled")
	}
	results, err := c.searchOrder(ctx, query, TV, tvOrder, tvKey)
	if err != nil {
		return nil, err
	}
	if lastErr != nil && len(results) == 0 {
		return nil, lastErr
	}
	return results, nil
}

// ProviderNames returns the registered provider names, for logging.
func (c *Chain) ProviderNames() []string {
	names := make([]string, 0, len(c.providers))
	for _, p := range c.providers {
		names = append(names, p.Name())
	}
	return names
}

var _ Provider = (*Chain)(nil)
