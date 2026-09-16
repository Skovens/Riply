# Plan: Enforce API Rate Limits for TMDb & OMDb

## Purpose
Ensure Riply never exceeds the documented rate limits of TMDb and OMDb APIs, preventing throttling (HTTP 429), quota exhaustion, and being flagged as abusive scraping.

## API Limits (Verified)

### TMDb (Free Tier)
- **No hard rate limit** (removed Dec 2019)
- **Soft limit**: ~40 requests/second — "be respectful of the service"
- **Honour 429 responses** — if received, back off immediately
- **Current code**: 4 req/sec sustained ✅ (well under soft limit)

### OMDb (Free Tier)
- **10 requests per minute** (per-minute throttle)
- **1,000 requests per month** (monthly quota)
- **Backoff**: exponential backoff with jitter, honour `Retry-After` header
- **Current code**: 10 req/sec sustained ❌ — **exceeds 10/min by 60x**

## Problems Found

| # | Problem | Severity | Impact |
|---|---------|----------|--------|
| 1 | OMDb limiter at 10/sec instead of 10/min | 🔴 Critical | Could exceed 600/min, triggering throttling |
| 2 | Infinite retries with no max attempts | 🔴 Critical | A single failing request could hammer the API indefinitely |
| 3 | No monthly quota tracking for OMDb | 🟡 High | Could silently exceed 1,000/month |
| 4 | No response caching | 🟡 Medium | Every scan re-calls APIs for the same data |
| 5 | No 429 handling | 🟡 Medium | Doesn't honour API's explicit throttle signal |

## Design

### 1. Rate Limiter Enhancement

Replace the current single-rate-limiter with a **per-service limiter** that supports:

- **Per-minute sliding window** (OMDb: 10/min, TMDb: 240/min conservative)
- **Monthly quota counter** (OMDb: 1,000/month, resets on 1st of each month)
- **Per-service independent tracking**

```rust
pub struct ServiceLimits {
    pub per_minute: u32,       // requests per minute (sliding window)
    pub per_month: u64,        // requests per month (calendar month)
    pub max_retries: u32,      // max retry attempts per request
    pub retry_max_delay: Duration, // cap on retry backoff
}

pub struct HttpClient {
    // ... existing fields ...
    tmdb_limits: ServiceLimits,
    omdb_limits: ServiceLimits,
    omdb_monthly_counter: AtomicU64, // reset on month boundary
    limiter: RateLimiter, // unified limiter that respects both windows
}
```

### 2. Retry Configuration

Replace `ExponentialBuilder::default()` (infinite retries) with:

```rust
ExponentialBuilder::default()
    .with_max_times(3)        // max 3 retry attempts
    .with_min_delay(Duration::from_millis(500))
    .with_max_delay(Duration::from_secs(10))
    .on_retry(|attempt, next_delay| {
        warn!(attempt, ?next_delay, "API request failed, retrying");
    })
```

### 3. 429 Handling

Detect HTTP 429 in response and:
1. Parse `Retry-After` header (seconds to wait)
2. If no `Retry-After`, use exponential backoff (500ms, 1s, 2s, 4s...)
3. Log the 429 with service name and endpoint
4. Increment a `throttled_count` metric

### 4. Response Caching

Add an in-memory LRU cache for search/enrichment results:

```rust
pub struct MetadataCache {
    search_cache: LruCache<SearchKey, Vec<MetadataMatch>>,
    enrichment_cache: LruCache<EnrichmentKey, MetadataEnrichment>,
}
```

Cache keys:
- Search: `(query, year, media_type)`
- Enrichment: `(tmdb_id, media_type)`

Cache TTL: 24 hours (metadata doesn't change)

### 5. Metrics & Logging

Track and log (on shutdown or every 100 requests):

```
omdb_requests_total=42, omdb_quota_remaining=958, omdb_minute_remaining=7
tmdb_requests_total=128
throttled_count=0
cache_hit_count=15, cache_miss_count=27
```

## Files to Modify

| File | Changes |
|------|---------|
| `infra/src/rate_limiter.rs` | Add per-minute sliding window + monthly quota support |
| `infra/src/http.rs` | Use new limiter, add max retries, 429 handling, caching |
| `core/src/metadata.rs` | Add cache key types if needed |

## Acceptance Criteria

- [ ] **AC1**: OMDb rate limiter enforces 10 requests/minute (sliding window)
- [ ] **AC2**: OMDb monthly quota tracking with 1,000/month limit — stops calls at quota
- [ ] **AC3**: All API retries capped at 3 attempts with max 10s delay
- [ ] **AC4**: HTTP 429 responses are detected, logged, and trigger backoff
- [ ] **AC5**: TMDb rate limiter stays at 4 req/sec (unchanged, verified correct)
- [ ] **AC6**: Metadata search/enrichment results cached in memory (24h TTL)
- [ ] **AC7**: Usage metrics logged (requests, quota remaining, cache hits)
- [ ] **AC8**: All 234 existing tests pass
- [ ] **AC9**: Clippy clean (0 warnings)
- [ ] **AC10**: Frontend tests pass (52/52)

## Implementation Order

1. **Rate limiter enhancement** — per-minute + monthly quota
2. **Retry configuration** — max attempts, delay caps
3. **429 handling** — detect, log, backoff
4. **Response caching** — LRU cache for search/enrichment
5. **Metrics & logging** — usage tracking
6. **Tests** — unit tests for new limiter behavior, cache, 429 handling
7. **Integration** — verify with `cargo test --all`, `cargo clippy`, `npm test`

## Testing Strategy

- **Unit tests**: Rate limiter with per-minute window, monthly quota exhaustion
- **Unit tests**: Retry with max attempts (verify it stops after N tries)
- **Unit tests**: 429 response handling (verify backoff)
- **Unit tests**: Cache hit/miss scenarios
- **Integration**: Run `riply library scan` on a directory, verify API call count

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Cache invalidation — TMDb data changes | 24h TTL is short enough; metadata rarely changes |
| Monthly quota reset — handling month boundary | Check if month changed on each request, reset counter |
| Per-minute sliding window — complexity | Use a simple token bucket with 60s refill period |
| Cache memory usage — large result sets | LRU with bounded size (e.g., 500 entries) |
