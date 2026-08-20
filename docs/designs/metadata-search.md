# Design: Metadata Search

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Search external metadata APIs (TMDb, OMDb) to identify media files and enrich matches with additional metadata. This document describes the *current* behavior of the metadata search system.

---

## Architecture

```
core (trait) → infra (HTTP adapter) → app (analyzer consumer)
```

- `MetadataSearcher` trait defined in `core/src/metadata.rs` — zero infrastructure dependencies
- `HttpClient` implements the trait in `infra/src/http.rs` — the only concrete provider
- `AnalyzerService` consumes the trait in `app/src/analyzer.rs`

---

## MetadataSearcher Trait

### Methods

```rust
pub trait MetadataSearcher: Send + Sync {
    // Search for movies by title and optional year
    fn search_movies(&self, query: &str, year: Option<u32>)
        -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_>>;

    // Search for TV shows by title
    fn search_tv(&self, query: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_>>;

    // Unified search — delegates to search_movies or search_tv based on media_type
    fn search(&self, query: &str, year: Option<u32>, media_type: &MediaType)
        -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_>>;

    // Fetch enrichment data (genres, runtime, rating, overview, cast)
    fn fetch_enrichment(&self, tmdb_id: i32, media_type: &MediaType)
        -> Pin<Box<dyn Future<Output = Result<MetadataEnrichment, MetadataError>> + Send + '_>>;
}
```

### Default Implementation

The `search()` method provides a default implementation that delegates to `search_movies()` or `search_tv()` based on `media_type`. The `fetch_enrichment()` method provides a default that returns an empty `MetadataEnrichment` so providers that don't support enrichment can skip implementation.

---

## TMDb Integration

### Configuration

- **Base URL:** `https://api.themoviedb.org/3`
- **Rate limit:** 40 requests per 4 seconds
- **API key:** Required — returns empty results if not configured

### Search Movie

**Endpoint:** `GET /search/movie?api_key={key}&query={title}&year={year}`

**Response parsing:**
```json
{
    "results": [
        {
            "id": 550,
            "title": "Fight Club",
            "release_date": "1999-10-15"
        }
    ]
}
```

**Output:**
- `title` from `title` field
- `year` parsed from `release_date` (first 4 chars)
- `tmdb_id` from `id`
- `is_tv = false`

### Search TV

**Endpoint:** `GET /search/tv?api_key={key}&query={title}`

**Response parsing:**
```json
{
    "results": [
        {
            "id": 1396,
            "name": "Breaking Bad",
            "first_air_date": "2008-01-20"
        }
    ]
}
```

**Output:**
- `title` from `name` field
- `year` parsed from `first_air_date` (first 4 chars)
- `tmdb_id` from `id`
- `is_tv = true`

### Enrichment

**Endpoint:** `GET /movie/{id}` or `GET /tv/{id}?append_to_response=credits,release_dates`

**Fields extracted:**

| Field | Path | Example |
|---|---|---|
| Genres | `genres[].name` | `["Drama", "Thriller"]` |
| Runtime | `runtime` | `139` (minutes) |
| Overview | `overview` | `"An insomniac office worker."` |
| Cast | `credits.cast[].name` | `["Brad Pitt", "Edward Norton"]` |
| Content Rating | `release_dates.results[country="US"].release_dates[].certification` | `"R"` |

**Fallback:** If no US release date found, `content_rating` is `None`.

---

## OMDb Integration

### Configuration

- **Base URL:** `http://www.omdbapi.com`
- **Rate limit:** 10 requests per 10 seconds
- **API key:** Required — returns empty results if not configured

### Search Movie

**Endpoint:** `GET /?apikey={key}&t={title}&y={year}`

**Response parsing:**
```json
{
    "Title": "The Matrix",
    "Year": "1999",
    "Response": "True"
}
```

**Output:**
- `title` from `Title` field
- `year` parsed from `Year`
- `is_tv = false` (OMDb doesn't distinguish)

### Search TV

**Endpoint:** `GET /?apikey={key}&s={title}&type=series`

**Response parsing:**
```json
{
    "Search": [
        { "Title": "Breaking Bad", "Year": "2008", "imdbID": "tt0903747" }
    ],
    "Response": "True"
}
```

**Output:**
- `title` from `Title` field
- `year` from `Year`
- `is_tv = true`

---

## Search Fallback Chain

### Movie Search

```
1. TMDb search_movies(query, year)
   └─ Success with results? → Return top 5
   └─ Empty results? → Fall through
   └─ Error? → Log warning, fall through
2. OMDb search(query, year)
   └─ Return results (up to 5)
```

### TV Search

```
1. TMDb search_tv(query)
   └─ Success with results? → Return top 5
   └─ Empty results? → Fall through
   └─ Error? → Log warning, fall through
2. OMDb search_tv(query, year)
   └─ Return results (up to 5)
```

### Key Rules

1. **TMDb is preferred** — OMDb is only a fallback
2. **Errors don't short-circuit** — TMDb errors are logged and OMDb is tried
3. **Empty results trigger fallback** — If TMDb returns 0 results, OMDb is tried
4. **Results are truncated to 5** — `results.truncate(5)` after the fallback chain

---

## Rate Limiting

### TMDb

- **Limit:** 40 requests per 4 seconds
- **Implementation:** `RateLimiter::new(40, 4.0)` — token bucket algorithm
- **Mechanism:** `limiter.acquire().await` before each request

### OMDb

- **Limit:** 10 requests per 10 seconds
- **Implementation:** `RateLimiter::new(10, 10.0)`
- **Mechanism:** Same token bucket

### Retry Logic

All HTTP requests use exponential backoff via `backon::Retryable`:
- Default exponential builder (starts at ~1s, doubles each retry)
- Retries on transient failures (connection errors, timeouts)
- Does NOT retry on 4xx client errors (rate limiting, auth failures)

---

## MetadataMatch

### Struct

```rust
pub struct MetadataMatch {
    pub title: String,           // Matched title
    pub year: Option<u32>,       // Matched year
    pub tmdb_id: Option<i32>,    // TMDb ID (None for OMDb results)
    pub is_tv: bool,             // Movie or TV
    pub series_name: Option<String>,  // TV series name (populated by analyzer)
    pub season_number: Option<u32>,   // TV season (populated by analyzer)
    pub episode_number: Option<u32>,  // TV episode (populated by analyzer)
    pub episode_title: Option<String>,// TV episode title (from TMDb episode details)
}
```

**Note:** `series_name`, `season_number`, `episode_number`, and `episode_title` are populated by the `AnalyzerService` after search, not by the search itself.

---

## MetadataEnrichment

### Struct

```rust
pub struct MetadataEnrichment {
    pub genres: Option<Vec<String>>,    // e.g. ["Drama", "Crime"]
    pub runtime: Option<i32>,           // Minutes
    pub content_rating: Option<String>, // e.g. "PG-13", "TV-MA"
    pub overview: Option<String>,       // Plot summary
    pub cast_list: Option<Vec<String>>, // Leading cast names
}
```

### Source

Enrichment is fetched exclusively from TMDb. OMDb results do not support enrichment — if an OMDb match is used, enrichment fields remain `None`.

---

## HTTP Client

### Configuration

```rust
pub struct HttpClient {
    client: Client,                    // reqwest client (10s timeout)
    tmdb_base_url: String,
    omdb_base_url: String,
    tmdb_api_key: Option<String>,
    omdb_api_key: Option<String>,
    tmdb_limiter: Option<RateLimiter>,
    omdb_limiter: Option<RateLimiter>,
}
```

### Construction

```rust
impl HttpClient {
    // Convenience constructor with default URLs
    pub fn new(tmdb_api_key: Option<String>, omdb_api_key: Option<String>) -> Self

    // Full control over URLs (for testing)
    fn with_urls(
        tmdb_base_url: String,
        omdb_base_url: String,
        tmdb_api_key: Option<String>,
        omdb_api_key: Option<String>,
    ) -> Self
}
```

### Error Handling

| Error Type | Behavior |
|---|---|
| No API key configured | Returns empty results (graceful degradation) |
| Network error | Retries with exponential backoff |
| Rate limit (429) | Retries with backoff |
| 4xx client error | Returns error (no retry) |
| Invalid JSON | Returns error |
| Empty response body | Returns error |

### MetadataError Enum

```rust
pub enum MetadataError {
    ApiError(String),     // Generic API error
    InvalidResponse(String), // Malformed response
    RateLimited,          // Rate limit exceeded
}
```

---

## Integration with Analyzer

### Search Flow in `analyze_file()`

```
1. Extract ffprobe metadata
2. Parse filename for title, year, episode info
3. If content_type != "extra" and searcher available:
   a. Call searcher.search(title, year, media_type)
   b. If results:
      - Take first match
      - Set suggested_title, suggested_year, tmdb_id
      - For TV: set series_name, season, episode, episode_title
      - Set confidence = 0.95, source = "tmdb"
      - Fetch enrichment (if tmdb_id available)
      - Set status = Matched
      - Return
   c. If error: log warning, fall through
4. Filename-only heuristic (fallback)
   - Score confidence based on title length, year, dots
   - TV episodes get min confidence 0.85
   - confidence >= 0.8 → Matched
   - confidence < 0.8 → NeedsReview
```

### Graceful Degradation

The metadata search layer is designed to fail silently:
- No API key → empty results → filename-only heuristic
- TMDb error → OMDb fallback → filename-only heuristic
- OMDb error → filename-only heuristic
- Enrichment failure → match still valid, enrichment fields remain `None`

---

## Where This Lives

| Concern | Layer |
|---|---|
| MetadataMatch struct | `core/src/metadata.rs` |
| MetadataEnrichment struct | `core/src/metadata.rs` |
| MetadataError enum | `core/src/metadata.rs` |
| MetadataSearcher trait | `core/src/metadata.rs` |
| HTTP client + TMDb + OMDb | `infra/src/http.rs` |
| Rate limiter | `infra/src/rate_limiter.rs` |
| Analyzer consumer | `app/src/analyzer.rs` |

---

## Notes

- **TMDb is the primary provider** — OMDb is a fallback for when TMDb returns no results
- **Rate limits are enforced** — TMDb: 40/4s, OMDb: 10/10s
- **Enrichment is TMDb-only** — OMDb results don't get genres/rating/cast
- **No API key = no search** — The system falls back to filename-only heuristics
- **Confidence scoring is heuristic-based** — TMDb matches get 0.95; filename-only matches score 0.5–1.0
- **0.8 threshold separates Matched from NeedsReview** — This is an arbitrary threshold that can be adjusted
- **Extras skip metadata search** — Commentary/behind-the-scenes content doesn't need title matching
- **Results truncated to 5** — Only the top 5 matches are returned to the UI
