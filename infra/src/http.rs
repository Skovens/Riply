use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use lru::LruCache;
use reqwest::Client;
use riply_core::{MediaType, MetadataEnrichment, MetadataError, MetadataMatch, MetadataSearcher};
use tracing::{info, warn};

use crate::rate_limiter::{
    MonthlyQuota, RateLimit, RateLimitPolicy, RateLimitRegistry, RateLimiter, SlidingWindowLimiter,
};

/// Maximum LRU cache size for metadata search/enrichment results.
const CACHE_SIZE: NonZeroUsize = NonZeroUsize::new(500).unwrap();

/// Cache entry with 24-hour expiration.
struct Cached<T> {
    value: T,
    expires_at: std::time::Instant,
}

impl<T> Cached<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            expires_at: std::time::Instant::now() + ttl,
        }
    }

    fn is_expired(&self) -> bool {
        std::time::Instant::now() >= self.expires_at
    }
}

/// Usage metrics for API calls and cache performance.
/// Separate from cache so it can be swapped independently.
#[derive(Default, Clone)]
pub struct UsageMetrics {
    pub search_hits: usize,
    pub search_misses: usize,
    pub enrichment_hits: usize,
    pub enrichment_misses: usize,
    pub total_requests: usize,
    pub throttled_events: usize,
}

impl UsageMetrics {
    /// Log cache and usage statistics to tracing.
    pub fn log_stats(&self) {
        info!(
            total_requests = self.total_requests,
            search_hits = self.search_hits,
            search_misses = self.search_misses,
            enrichment_hits = self.enrichment_hits,
            enrichment_misses = self.enrichment_misses,
            throttled_events = self.throttled_events,
            "Metadata API usage statistics"
        );
    }
}

/// Metadata cache with 24-hour TTL. Pure LRU storage, no metrics.
struct MetadataCache {
    search: LruCache<(String, Option<u32>, bool), Cached<Vec<MetadataMatch>>>,
    enrichment: LruCache<(i32, bool), Cached<MetadataEnrichment>>,
}

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours

impl MetadataCache {
    fn new() -> Self {
        MetadataCache {
            search: LruCache::new(CACHE_SIZE),
            enrichment: LruCache::new(CACHE_SIZE),
        }
    }

    fn search_key(query: &str, year: Option<u32>, is_tv: bool) -> (String, Option<u32>, bool) {
        (query.to_string(), year, is_tv)
    }

    fn enrichment_key(tmdb_id: i32, is_tv: bool) -> (i32, bool) {
        (tmdb_id, is_tv)
    }

    fn get_search(&mut self, key: &(String, Option<u32>, bool)) -> Option<Vec<MetadataMatch>> {
        if let Some(cached) = self.search.pop(key) {
            if cached.is_expired() {
                return None;
            }
            Some(cached.value)
        } else {
            None
        }
    }

    fn put_search(&mut self, key: (String, Option<u32>, bool), value: Vec<MetadataMatch>) {
        self.search.put(key, Cached::new(value, CACHE_TTL));
    }

    fn get_enrichment(&mut self, key: &(i32, bool)) -> Option<MetadataEnrichment> {
        if let Some(cached) = self.enrichment.pop(key) {
            if cached.is_expired() {
                return None;
            }
            Some(cached.value)
        } else {
            None
        }
    }

    fn put_enrichment(&mut self, key: (i32, bool), value: MetadataEnrichment) {
        self.enrichment.put(key, Cached::new(value, CACHE_TTL));
    }
}

/// HTTP client adapter - handles API requests with retry and rate limiting
#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    tmdb_base_url: String,
    omdb_base_url: String,
    tmdb_api_key: Option<String>,
    omdb_api_key: Option<String>,
    registry: RateLimitRegistry,
    cache: Arc<tokio::sync::Mutex<MetadataCache>>,
    metrics: Arc<tokio::sync::Mutex<UsageMetrics>>,
}

/// TMDb search result
#[derive(Debug, serde::Deserialize)]
struct TmdbSearchResponse {
    results: Vec<TmdbResult>,
}

/// Single TMDb search result
#[derive(Debug, serde::Deserialize)]
struct TmdbResult {
    pub id: i32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub first_air_date: Option<String>,
}

/// OMDb search result
#[derive(Debug, serde::Deserialize)]
struct OmdbResponse {
    #[serde(default, rename = "Title")]
    title: Option<String>,
    #[serde(default, rename = "Year")]
    year: Option<String>,
    #[serde(default, rename = "Response")]
    response: Option<String>,
}

/// TMDb title details response (with credits and release dates appended).
#[derive(Debug, Default, serde::Deserialize)]
struct TmdbDetails {
    #[serde(default)]
    genres: Vec<TmdbGenre>,
    #[serde(default)]
    runtime: Option<i32>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    credits: Option<TmdbCredits>,
    #[serde(default)]
    release_dates: Option<TmdbReleaseDates>,
}

#[derive(Debug, serde::Deserialize)]
struct TmdbGenre {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct TmdbCredits {
    #[serde(default)]
    cast: Vec<TmdbCastMember>,
}

#[derive(Debug, serde::Deserialize)]
struct TmdbCastMember {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct TmdbReleaseDates {
    #[serde(default)]
    results: Vec<TmdbReleaseCountry>,
}

#[derive(Debug, serde::Deserialize)]
struct TmdbReleaseCountry {
    #[serde(default)]
    iso_3166_1: Option<String>,
    #[serde(default)]
    release_dates: Vec<TmdbReleaseDate>,
}

#[derive(Debug, serde::Deserialize)]
struct TmdbReleaseDate {
    #[serde(default)]
    certification: Option<String>,
}

/// TMDb episode detail result
#[derive(Debug, serde::Deserialize)]
pub struct TmdbEpisodeDetail {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub season_number: Option<u32>,
    #[serde(default)]
    pub episode_number: Option<u32>,
    #[serde(default)]
    pub air_date: Option<String>,
    /// Runtime in minutes (TMDb returns episode runtime in minutes).
    #[serde(default)]
    pub runtime: Option<i32>,
}

/// Extract the US content rating from TMDb release dates.
fn find_us_certification(release_dates: &Option<TmdbReleaseDates>) -> Option<String> {
    let rd = release_dates.as_ref()?;
    let us = rd
        .results
        .iter()
        .find(|c| c.iso_3166_1.as_deref() == Some("US"))?;
    let cert = us
        .release_dates
        .iter()
        .find(|d| d.certification.as_deref().is_some_and(|s| !s.is_empty()))?;
    cert.certification.clone()
}

impl HttpClient {
    pub fn new(tmdb_api_key: Option<String>, omdb_api_key: Option<String>) -> Self {
        info!("Initializing HTTP client");
        Self::with_urls(
            "https://api.themoviedb.org/3".to_string(),
            "http://www.omdbapi.com".to_string(),
            tmdb_api_key,
            omdb_api_key,
        )
    }

    fn with_urls(
        tmdb_base_url: String,
        omdb_base_url: String,
        tmdb_api_key: Option<String>,
        omdb_api_key: Option<String>,
    ) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        // Build rate limit registry
        let mut registry = RateLimitRegistry::new();
        if let Some(ref _key) = tmdb_api_key {
            // TMDb: 4 req/sec token bucket (well under soft limit of ~40/sec)
            registry.register(
                "https://api.themoviedb.org/3",
                RateLimitPolicy::TokenBucket(Arc::new(RateLimiter::new(40, 4.0))),
            );
        }
        if let Some(ref _key) = omdb_api_key {
            // OMDb: 10 req/min sliding window (official limit)
            registry.register(
                "http://www.omdbapi.com",
                RateLimitPolicy::SlidingWindow(Arc::new(SlidingWindowLimiter::new(10, 60))),
            );
            // OMDb: 1,000 req/month quota
            registry.register_quota(RateLimitPolicy::MonthlyQuota(Arc::new(MonthlyQuota::new(
                1_000,
            ))));
        }

        HttpClient {
            client,
            tmdb_base_url,
            omdb_base_url,
            tmdb_api_key,
            omdb_api_key,
            registry,
            cache: Arc::new(tokio::sync::Mutex::new(MetadataCache::new())),
            metrics: Arc::new(tokio::sync::Mutex::new(UsageMetrics::default())),
        }
    }

    #[cfg(test)]
    pub fn for_testing(
        base_url: String,
        tmdb_api_key: Option<String>,
        omdb_api_key: Option<String>,
    ) -> Self {
        Self::with_urls(base_url.clone(), base_url, tmdb_api_key, omdb_api_key)
    }

    /// Perform a GET request with rate limiting and retries.
    /// Returns raw response; caller handles deserialization and 429.
    async fn get_json(
        &self,
        url: &str,
        params: &HashMap<String, String>,
    ) -> Result<reqwest::Response, anyhow::Error> {
        // Apply quota policies (apply to all APIs)
        for quota in self.registry.quotas() {
            if let RateLimitPolicy::MonthlyQuota(ref q) = quota {
                q.try_acquire()
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }
        }

        // Apply rate limiter from registry (by URL prefix match)
        if let Some(policy) = self.registry.get(url) {
            policy.acquire().await;
        }

        let url = url.to_string();
        let params = params.clone();
        let client = self.client.clone();

        let resp = (move || {
            let client = client.clone();
            let url = url.clone();
            let params = params.clone();
            async move { client.get(&url).query(&params).send().await }
        })
        .retry(
            ExponentialBuilder::default()
                .with_max_times(3)
                .with_min_delay(Duration::from_millis(500))
                .with_max_delay(Duration::from_secs(10)),
        )
        .when(|error: &reqwest::Error| {
            // Retry on network errors, not on HTTP errors
            error.is_connect() || error.is_timeout()
        })
        .await
        .map_err(anyhow::Error::from)?;

        Ok(resp)
    }

    /// Handle a 429 response: record throttle, back off, retry once.
    /// Caller must deserialize the response after calling this.
    async fn handle_429(
        &self,
        resp: reqwest::Response,
        url: &str,
        params: &HashMap<String, String>,
    ) -> Result<reqwest::Response, anyhow::Error> {
        let mut metrics = self.metrics.lock().await;
        metrics.throttled_events += 1;
        drop(metrics);

        let retry_after = resp
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);

        warn!(
            retry_after_secs = retry_after,
            "API rate limited (429), backing off"
        );

        tokio::time::sleep(Duration::from_secs(retry_after)).await;

        // One final attempt after backoff
        let resp = self.client.get(url).query(params).send().await?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(anyhow::anyhow!("API rate limited after backoff (429)"));
        }

        Ok(resp)
    }

    /// Handle HTTP status: 429 triggers backoff retry, success deserializes.
    async fn handle_http_status(
        &self,
        resp: reqwest::Response,
        url: &str,
        params: &HashMap<String, String>,
    ) -> Result<reqwest::Response, anyhow::Error> {
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return self.handle_429(resp, url, params).await;
        }
        Ok(resp)
    }

    /// Deserialize a response, recording usage metrics.
    async fn record_and_deserialize<T: serde::de::DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, anyhow::Error> {
        let mut metrics = self.metrics.lock().await;
        metrics.total_requests += 1;
        drop(metrics);
        Ok(resp.json::<T>().await?)
    }

    pub async fn search_tmdb_movies(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> Result<Vec<MetadataMatch>, anyhow::Error> {
        let api_key = match &self.tmdb_api_key {
            Some(key) => key.clone(),
            None => return Ok(vec![]),
        };

        let mut params = HashMap::new();
        params.insert("api_key".to_string(), api_key);
        params.insert("query".to_string(), query.to_string());
        if let Some(y) = year {
            params.insert("year".to_string(), y.to_string());
        }

        let url = format!("{}/search/movie", self.tmdb_base_url);
        let resp = self.get_json(&url, &params).await?;
        let resp = self.handle_http_status(resp, &url, &params).await?;
        let resp: TmdbSearchResponse = self.record_and_deserialize(resp).await?;

        let matches = resp
            .results
            .into_iter()
            .map(|r| MetadataMatch {
                title: r.title.unwrap_or_default(),
                year: r
                    .release_date
                    .as_ref()
                    .and_then(|d| d.split('-').next()?.parse().ok()),
                tmdb_id: Some(r.id),
                is_tv: false,
                series_name: None,
                season_number: None,
                episode_number: None,
                episode_title: None,
            })
            .collect();

        Ok(matches)
    }

    pub async fn search_tmdb_tv(&self, query: &str) -> Result<Vec<MetadataMatch>, anyhow::Error> {
        let api_key = match &self.tmdb_api_key {
            Some(key) => key.clone(),
            None => return Ok(vec![]),
        };

        let mut params = HashMap::new();
        params.insert("api_key".to_string(), api_key);
        params.insert("query".to_string(), query.to_string());

        let url = format!("{}/search/tv", self.tmdb_base_url);
        let resp = self.get_json(&url, &params).await?;
        let resp = self.handle_http_status(resp, &url, &params).await?;
        let resp: TmdbSearchResponse = self.record_and_deserialize(resp).await?;

        let matches = resp
            .results
            .into_iter()
            .map(|r| MetadataMatch {
                title: r.name.unwrap_or_default(),
                year: r
                    .first_air_date
                    .as_ref()
                    .and_then(|d| d.split('-').next()?.parse().ok()),
                tmdb_id: Some(r.id),
                is_tv: true,
                series_name: None,
                season_number: None,
                episode_number: None,
                episode_title: None,
            })
            .collect();

        Ok(matches)
    }

    pub async fn search_omdb(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> Result<Vec<MetadataMatch>, anyhow::Error> {
        let api_key = match &self.omdb_api_key {
            Some(key) => key.clone(),
            None => return Ok(vec![]),
        };

        let mut params = HashMap::new();
        params.insert("apikey".to_string(), api_key);
        params.insert("t".to_string(), query.to_string());
        if let Some(y) = year {
            params.insert("y".to_string(), y.to_string());
        }

        let url = format!("{}/", self.omdb_base_url);
        let resp = self.get_json(&url, &params).await?;
        let resp = self.handle_http_status(resp, &url, &params).await?;
        let resp: OmdbResponse = self.record_and_deserialize(resp).await?;

        if resp.response.as_deref() == Some("True") {
            let year = resp.year.as_ref().and_then(|y| y.trim().parse().ok());
            return Ok(vec![MetadataMatch {
                title: resp.title.unwrap_or_default(),
                year,
                tmdb_id: None,
                is_tv: false,
                series_name: None,
                season_number: None,
                episode_number: None,
                episode_title: None,
            }]);
        }

        Ok(vec![])
    }

    /// Search OMDb for TV series
    pub async fn search_omdb_tv(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> Result<Vec<MetadataMatch>, anyhow::Error> {
        let api_key = match &self.omdb_api_key {
            Some(key) => key.clone(),
            None => return Ok(vec![]),
        };

        let mut params = HashMap::new();
        params.insert("apikey".to_string(), api_key);
        params.insert("t".to_string(), query.to_string());
        params.insert("type".to_string(), "series".to_string());
        if let Some(y) = year {
            params.insert("y".to_string(), y.to_string());
        }

        let url = format!("{}/", self.omdb_base_url);
        let resp = self.get_json(&url, &params).await?;
        let resp = self.handle_http_status(resp, &url, &params).await?;
        let resp: OmdbResponse = self.record_and_deserialize(resp).await?;

        if resp.response.as_deref() == Some("True") {
            let year = resp.year.as_ref().and_then(|y| y.trim().parse().ok());
            return Ok(vec![MetadataMatch {
                title: resp.title.unwrap_or_default(),
                year,
                tmdb_id: None,
                is_tv: true,
                series_name: None,
                season_number: None,
                episode_number: None,
                episode_title: None,
            }]);
        }

        Ok(vec![])
    }

    /// Fetch episode details from TMDb for a specific TV show season/episode.
    /// Requires the TMDb show ID, season number, and episode number.
    pub async fn fetch_tmdb_episode(
        &self,
        tmdb_id: i32,
        season: u32,
        episode: u32,
    ) -> Result<Option<TmdbEpisodeDetail>, anyhow::Error> {
        let api_key = match &self.tmdb_api_key {
            Some(key) => key.clone(),
            None => return Ok(None),
        };

        let url = format!(
            "{}/tv/{}/season/{}/episode/{}",
            self.tmdb_base_url, tmdb_id, season, episode
        );
        let params = HashMap::from([("api_key".to_string(), api_key)]);

        let resp: reqwest::Response = {
            let url = url.clone();
            let params = params.clone();
            let client = self.client.clone();
            (move || {
                let client = client.clone();
                let url = url.clone();
                let params = params.clone();
                async move { client.get(&url).query(&params).send().await }
            })
            .retry(ExponentialBuilder::default())
            .await?
        };

        if !resp.status().is_success() {
            return Ok(None);
        }

        let detail: TmdbEpisodeDetail = resp.json().await?;
        Ok(Some(detail))
    }

    /// Fetch the expected duration (in seconds) for a TV episode from TMDb.
    /// Returns `None` if the episode runtime is not available or no API key is configured.
    pub async fn fetch_episode_duration(
        &self,
        tmdb_id: i32,
        season: u32,
        episode: u32,
    ) -> Result<Option<f64>, anyhow::Error> {
        match self.fetch_tmdb_episode(tmdb_id, season, episode).await? {
            Some(detail) => Ok(detail.runtime.map(|r| (r as f64) * 60.0)),
            None => Ok(None),
        }
    }

    /// Fetch enrichment metadata (genres, runtime, overview, cast, content
    /// rating) for a TMDb title. Uses `append_to_response` to bundle credits
    /// and release dates into one request.
    pub async fn fetch_tmdb_enrichment(
        &self,
        tmdb_id: i32,
        media_type: &MediaType,
    ) -> Result<MetadataEnrichment, anyhow::Error> {
        let api_key = match &self.tmdb_api_key {
            Some(key) => key.clone(),
            None => return Ok(MetadataEnrichment::default()),
        };

        let kind = if media_type.is_tv() { "tv" } else { "movie" };
        let url = format!("{}/{}/{}", self.tmdb_base_url, kind, tmdb_id);
        let params = HashMap::from([
            ("api_key".to_string(), api_key),
            (
                "append_to_response".to_string(),
                "credits,release_dates".to_string(),
            ),
        ]);

        let resp = self.get_json(&url, &params).await?;
        let resp = self.handle_http_status(resp, &url, &params).await?;
        let resp: TmdbDetails = self.record_and_deserialize(resp).await?;

        let genres = (!resp.genres.is_empty())
            .then(|| {
                resp.genres
                    .into_iter()
                    .filter_map(|g| g.name)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty());

        let cast_list = resp
            .credits
            .as_ref()
            .map(|c| {
                c.cast
                    .iter()
                    .filter_map(|m| m.name.clone())
                    .take(5)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty());

        let content_rating = find_us_certification(&resp.release_dates);

        Ok(MetadataEnrichment {
            genres,
            runtime: resp.runtime.filter(|r| *r > 0),
            content_rating,
            overview: resp.overview.filter(|o| !o.is_empty()),
            cast_list,
        })
    }
}

impl MetadataSearcher for HttpClient {
    fn search_movies(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_,
        >,
    > {
        let query = query.to_string();
        let client = self.clone();
        Box::pin(async move {
            let key = MetadataCache::search_key(&query, year, false);
            {
                let mut cache = client.cache.lock().await;
                if let Some(cached) = cache.get_search(&key) {
                    client.metrics.lock().await.search_hits += 1;
                    return Ok(cached);
                }
                client.metrics.lock().await.search_misses += 1;
            }
            let results = client
                .search_tmdb_movies(&query, year)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))?;
            let mut cache = client.cache.lock().await;
            cache.put_search(key, results.clone());
            Ok(results)
        })
    }

    fn search_tv(
        &self,
        query: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_,
        >,
    > {
        let query = query.to_string();
        let client = self.clone();
        Box::pin(async move {
            let key = MetadataCache::search_key(&query, None, true);
            {
                let mut cache = client.cache.lock().await;
                if let Some(cached) = cache.get_search(&key) {
                    client.metrics.lock().await.search_hits += 1;
                    return Ok(cached);
                }
                client.metrics.lock().await.search_misses += 1;
            }
            let results = client
                .search_tmdb_tv(&query)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))?;
            let mut cache = client.cache.lock().await;
            cache.put_search(key, results.clone());
            Ok(results)
        })
    }

    fn search(
        &self,
        query: &str,
        year: Option<u32>,
        media_type: &MediaType,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_,
        >,
    > {
        let query = query.to_string();
        let media_type = media_type.clone();
        let client = self.clone();
        Box::pin(async move {
            let key = MetadataCache::search_key(&query, year, media_type.is_tv());
            {
                let mut cache = client.cache.lock().await;
                if let Some(cached) = cache.get_search(&key) {
                    client.metrics.lock().await.search_hits += 1;
                    return Ok(cached);
                }
                client.metrics.lock().await.search_misses += 1;
            }
            let mut results = match &media_type {
                MediaType::Tv => {
                    let mut tv = match client.search_tmdb_tv(&query).await {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::warn!(error = %e, query = %query, "TMDb TV search failed, falling back to OMDb");
                            vec![]
                        }
                    };
                    if tv.is_empty() {
                        tv = client
                            .search_omdb_tv(&query, year)
                            .await
                            .map_err(|e| MetadataError::ApiError(e.to_string()))?;
                    }
                    tv
                }
                MediaType::Movie => {
                    let mut movies = match client.search_tmdb_movies(&query, year).await {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::warn!(error = %e, query = %query, "TMDb movie search failed, falling back to OMDb");
                            vec![]
                        }
                    };
                    if movies.is_empty() {
                        movies = client
                            .search_omdb(&query, year)
                            .await
                            .map_err(|e| MetadataError::ApiError(e.to_string()))?;
                    }
                    movies
                }
            };
            results.truncate(5);
            let mut cache = client.cache.lock().await;
            cache.put_search(key, results.clone());
            Ok(results)
        })
    }

    fn fetch_enrichment(
        &self,
        tmdb_id: i32,
        media_type: &MediaType,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<MetadataEnrichment, MetadataError>> + Send + '_,
        >,
    > {
        let media_type = media_type.clone();
        let client = self.clone();
        Box::pin(async move {
            let key = MetadataCache::enrichment_key(tmdb_id, media_type.is_tv());
            {
                let mut cache = client.cache.lock().await;
                if let Some(cached) = cache.get_enrichment(&key) {
                    client.metrics.lock().await.enrichment_hits += 1;
                    return Ok(cached);
                }
                client.metrics.lock().await.enrichment_misses += 1;
            }
            let result = client
                .fetch_tmdb_enrichment(tmdb_id, &media_type)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))?;
            let mut cache = client.cache.lock().await;
            cache.put_enrichment(key, result.clone());
            Ok(result)
        })
    }

    /// Log cache and usage statistics to tracing (async, non-blocking).
    fn log_stats(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let metrics = self.metrics.clone();
        Box::pin(async move {
            let m = metrics.lock().await;
            m.log_stats();
        })
    }

    fn fetch_episode_duration(
        &self,
        tmdb_id: i32,
        season: u32,
        episode: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<f64>, MetadataError>> + Send + '_>,
    > {
        let client = self.clone();
        Box::pin(async move {
            client
                .fetch_episode_duration(tmdb_id, season, episode)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riply_core::MetadataSearcher;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_tmdb_movie_search() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(".*/search/movie.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {
                        "id": 550,
                        "title": "Fight Club",
                        "release_date": "1999-10-15"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), Some("test_key".to_string()), None);

        let results = client
            .search_movies("Fight Club", Some(1999))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Fight Club");
        assert_eq!(results[0].year, Some(1999));
        assert_eq!(results[0].tmdb_id, Some(550));
        assert!(!results[0].is_tv);
    }

    #[tokio::test]
    async fn test_tmdb_tv_search() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(".*/search/tv.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {
                        "id": 1396,
                        "name": "Breaking Bad",
                        "first_air_date": "2008-01-20"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), Some("test_key".to_string()), None);

        let results = client.search_tv("Breaking Bad").await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Breaking Bad");
        assert_eq!(results[0].year, Some(2008));
        assert_eq!(results[0].tmdb_id, Some(1396));
        assert!(results[0].is_tv);
    }

    #[tokio::test]
    async fn test_omdb_search() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(".*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Title": "The Matrix",
                "Year": "1999",
                "Response": "True"
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), None, Some("test_key".to_string()));

        let results = client.search_omdb("The Matrix", Some(1999)).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "The Matrix");
        assert_eq!(results[0].year, Some(1999));
    }

    #[tokio::test]
    async fn test_tmdb_enrichment() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(".*/movie/550.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "genres": [{ "name": "Drama" }, { "name": "Thriller" }],
                "runtime": 139,
                "overview": "An insomniac office worker.",
                "credits": {
                    "cast": [
                        { "name": "Brad Pitt" },
                        { "name": "Edward Norton" },
                        { "name": "Helena Bonham Carter" }
                    ]
                },
                "release_dates": {
                    "results": [
                        { "iso_3166_1": "US", "release_dates": [{ "certification": "R" }] }
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), Some("test_key".to_string()), None);

        let enrichment = client
            .fetch_enrichment(550, &MediaType::Movie)
            .await
            .unwrap();

        assert_eq!(
            enrichment.genres,
            Some(vec!["Drama".to_string(), "Thriller".to_string()])
        );
        assert_eq!(enrichment.runtime, Some(139));
        assert_eq!(enrichment.content_rating.as_deref(), Some("R"));
        assert_eq!(
            enrichment.overview.as_deref(),
            Some("An insomniac office worker.")
        );
        assert_eq!(
            enrichment.cast_list.as_deref(),
            Some(
                &[
                    "Brad Pitt".to_string(),
                    "Edward Norton".to_string(),
                    "Helena Bonham Carter".to_string()
                ][..]
            )
        );
    }

    #[tokio::test]
    async fn test_no_api_key_returns_empty() {
        let client = HttpClient::new(None, None);

        let results = client.search_movies("Fight Club", None).await.unwrap();
        assert!(results.is_empty());

        let results = client.search_tv("Breaking Bad").await.unwrap();
        assert!(results.is_empty());

        let results = client.search_omdb("The Matrix", None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_cache_hit() {
        let mock_server = MockServer::start().await;

        // Only first call should hit the mock server
        Mock::given(method("GET"))
            .and(path_regex(".*/search/movie.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {
                        "id": 550,
                        "title": "Fight Club",
                        "release_date": "1999-10-15"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), Some("test_key".to_string()), None);

        // First call — cache miss, hits the mock server
        let results1 = client
            .search_movies("Fight Club", Some(1999))
            .await
            .unwrap();
        assert_eq!(results1.len(), 1);
        assert_eq!(results1[0].title, "Fight Club");

        // Second call — cache hit, should return same result without hitting server
        let results2 = client
            .search_movies("Fight Club", Some(1999))
            .await
            .unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].title, "Fight Club");

        // Verify cache stats
        let metrics = client.metrics.lock().await;
        assert_eq!(metrics.search_hits, 1);
        assert_eq!(metrics.search_misses, 1);
    }

    #[tokio::test]
    async fn test_search_cache_miss_different_year() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(".*/search/movie.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {
                        "id": 550,
                        "title": "Fight Club",
                        "release_date": "1999-10-15"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), Some("test_key".to_string()), None);

        // Different year = different cache key
        let results1 = client
            .search_movies("Fight Club", Some(1999))
            .await
            .unwrap();
        assert_eq!(results1.len(), 1);

        let results2 = client
            .search_movies("Fight Club", Some(2000))
            .await
            .unwrap();
        assert_eq!(results2.len(), 1);

        let metrics = client.metrics.lock().await;
        assert_eq!(metrics.search_hits, 0);
        assert_eq!(metrics.search_misses, 2);
    }

    #[tokio::test]
    async fn test_enrichment_cache_hit() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(".*/movie/550.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "genres": [{ "name": "Drama" }],
                "runtime": 139,
                "overview": "An insomniac office worker.",
                "credits": { "cast": [{"name": "Brad Pitt"}] },
                "release_dates": {
                    "results": [
                        { "iso_3166_1": "US", "release_dates": [{ "certification": "R" }] }
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), Some("test_key".to_string()), None);

        // Use trait method to exercise cache logic
        let enrichment1 =
            riply_core::MetadataSearcher::fetch_enrichment(&client, 550, &MediaType::Movie)
                .await
                .unwrap();
        assert_eq!(enrichment1.runtime, Some(139));

        // Second call — cache hit
        let enrichment2 =
            riply_core::MetadataSearcher::fetch_enrichment(&client, 550, &MediaType::Movie)
                .await
                .unwrap();
        assert_eq!(enrichment2.runtime, Some(139));

        let metrics = client.metrics.lock().await;
        assert_eq!(metrics.enrichment_hits, 1);
        assert_eq!(metrics.enrichment_misses, 1);
    }

    #[tokio::test]
    async fn test_cache_metrics_recording() {
        let client = HttpClient::new(Some("test_key".to_string()), None);

        // Initial state
        {
            let metrics = client.metrics.lock().await;
            assert_eq!(metrics.total_requests, 0);
            assert_eq!(metrics.throttled_events, 0);
        }

        // Manually record a request
        {
            let mut metrics = client.metrics.lock().await;
            metrics.total_requests += 1;
            assert_eq!(metrics.total_requests, 1);
        }

        // Manually record a throttle
        {
            let mut metrics = client.metrics.lock().await;
            metrics.throttled_events += 1;
            assert_eq!(metrics.throttled_events, 1);
        }

        // Verify stats logging works (now async, no blocking_lock needed)
        use riply_core::MetadataSearcher;
        let client2 = client.clone();
        client2.log_stats().await;
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(".*/search/movie.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {
                        "id": 550,
                        "title": "Fight Club",
                        "release_date": "1999-10-15"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::for_testing(mock_server.uri(), Some("test_key".to_string()), None);

        // First call — cache miss
        let results1 = client
            .search_movies("Fight Club", Some(1999))
            .await
            .unwrap();
        assert_eq!(results1.len(), 1);

        // Manually expire the cache entry
        {
            let mut cache = client.cache.lock().await;
            let key = ("Fight Club".to_string(), Some(1999), false);
            if let Some(cached) = cache.search.pop(&key) {
                let expired = Cached {
                    value: cached.value,
                    expires_at: std::time::Instant::now() - Duration::from_secs(1),
                };
                cache.search.put(key, expired);
            }
        }

        // Second call — cache expired, should hit server again
        let results2 = client
            .search_movies("Fight Club", Some(1999))
            .await
            .unwrap();
        assert_eq!(results2.len(), 1);

        let metrics = client.metrics.lock().await;
        assert_eq!(metrics.search_hits, 0);
        assert_eq!(metrics.search_misses, 2);
    }
}
