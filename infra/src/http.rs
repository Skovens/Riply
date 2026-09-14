use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use lru::LruCache;
use reqwest::Client;
use riply_core::{MediaType, MetadataEnrichment, MetadataError, MetadataMatch, MetadataSearcher};
use tracing::{info, warn};

use crate::rate_limiter::{MonthlyQuota, RateLimiter, SlidingWindowLimiter};

/// Maximum LRU cache size for metadata search/enrichment results.
const CACHE_SIZE: NonZeroUsize = NonZeroUsize::new(500).unwrap();

/// Metadata cache with 24-hour TTL.
struct MetadataCache {
    search: LruCache<(String, Option<u32>, bool), Vec<MetadataMatch>>,
    enrichment: LruCache<(i32, bool), MetadataEnrichment>,
    search_hits: usize,
    search_misses: usize,
    enrichment_hits: usize,
    enrichment_misses: usize,
}

impl MetadataCache {
    fn new() -> Self {
        MetadataCache {
            search: LruCache::new(CACHE_SIZE),
            enrichment: LruCache::new(CACHE_SIZE),
            search_hits: 0,
            search_misses: 0,
            enrichment_hits: 0,
            enrichment_misses: 0,
        }
    }

    fn search_key(query: &str, year: Option<u32>, is_tv: bool) -> (String, Option<u32>, bool) {
        (query.to_string(), year, is_tv)
    }

    fn enrichment_key(tmdb_id: i32, is_tv: bool) -> (i32, bool) {
        (tmdb_id, is_tv)
    }

    fn get_search(
        &mut self,
        key: &(String, Option<u32>, bool),
    ) -> Option<Vec<MetadataMatch>> {
        self.search.pop(key)
    }

    fn put_search(
        &mut self,
        key: (String, Option<u32>, bool),
        value: Vec<MetadataMatch>,
    ) {
        self.search.put(key, value);
    }

    fn record_search_hit(&mut self) {
        self.search_hits += 1;
    }

    fn record_search_miss(&mut self) {
        self.search_misses += 1;
    }

    fn get_enrichment(
        &mut self,
        key: &(i32, bool),
    ) -> Option<MetadataEnrichment> {
        self.enrichment.pop(key)
    }

    fn put_enrichment(
        &mut self,
        key: (i32, bool),
        value: MetadataEnrichment,
    ) {
        self.enrichment.put(key, value);
    }

    fn record_enrichment_hit(&mut self) {
        self.enrichment_hits += 1;
    }

    fn record_enrichment_miss(&mut self) {
        self.enrichment_misses += 1;
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
    tmdb_limiter: Option<Arc<RateLimiter>>,
    omdb_limiter: Option<Arc<SlidingWindowLimiter>>,
    omdb_quota: Option<Arc<MonthlyQuota>>,
    cache: Arc<std::sync::Mutex<MetadataCache>>,
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

        // TMDb: 4 req/sec token bucket (well under soft limit of ~40/sec)
        let tmdb_limiter = tmdb_api_key.is_some().then(|| Arc::new(RateLimiter::new(40, 4.0)));
        // OMDb: 10 req/min sliding window (official limit)
        let omdb_limiter = omdb_api_key.is_some().then(|| Arc::new(SlidingWindowLimiter::new(10, 60)));
        // OMDb: 1,000 req/month quota
        let omdb_quota = omdb_api_key.is_some().then(|| Arc::new(MonthlyQuota::new(1_000)));

        HttpClient {
            client,
            tmdb_base_url,
            omdb_base_url,
            tmdb_api_key,
            omdb_api_key,
            tmdb_limiter,
            omdb_limiter,
            omdb_quota,
            cache: Arc::new(std::sync::Mutex::new(MetadataCache::new())),
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

    /// Perform a GET request with rate limiting, retries, and 429 handling.
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        params: &HashMap<String, String>,
        limiter: Option<&RateLimiter>,
        omdb_limiter: Option<&SlidingWindowLimiter>,
        omdb_quota: Option<&Arc<MonthlyQuota>>,
    ) -> Result<T, anyhow::Error> {
        // OMDb monthly quota check
        if let Some(quota) = omdb_quota {
            quota.try_acquire().await.map_err(|e| anyhow::anyhow!("{}", e))?;
        }

        // Apply rate limiting
        if let Some(limiter) = limiter {
            limiter.acquire().await;
        }
        if let Some(limiter) = omdb_limiter {
            limiter.acquire().await;
        }

        let url = url.to_string();
        let params = params.clone();
        let fallback_url = url.clone();
        let fallback_params = params.clone();
        let retry_client = self.client.clone();
        let fallback_client = self.client.clone();

        let resp = (move || {
            let client = retry_client.clone();
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
        .await?;

        // Handle 429 - retry already handled network errors
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
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
            let resp = fallback_client
                .get(&fallback_url)
                .query(&fallback_params)
                .send()
                .await?;

            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(anyhow::anyhow!("API rate limited after backoff (429)"));
            }

            Ok(resp.json::<T>().await?)
        } else {
            Ok(resp.json::<T>().await?)
        }
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
        let resp: TmdbSearchResponse = self
            .get_json(&url, &params, self.tmdb_limiter.as_deref(), None, None)
            .await?;

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
        let resp: TmdbSearchResponse = self
            .get_json(&url, &params, self.tmdb_limiter.as_deref(), None, None)
            .await?;

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
        let resp: OmdbResponse = self
            .get_json(&url, &params, None, self.omdb_limiter.as_deref(), self.omdb_quota.as_ref())
            .await?;

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
        let resp: OmdbResponse = self
            .get_json(&url, &params, None, self.omdb_limiter.as_deref(), self.omdb_quota.as_ref())
            .await?;

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

        if let Some(limiter) = &self.tmdb_limiter {
            limiter.acquire().await;
        }

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

        let resp: TmdbDetails = self
            .get_json(&url, &params, self.tmdb_limiter.as_deref(), None, None)
            .await?;

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

        let content_rating = resp
            .release_dates
            .as_ref()
            .and_then(|rd| {
                rd.results
                    .iter()
                    .find(|c| c.iso_3166_1.as_deref() == Some("US"))
            })
            .and_then(|c| {
                c.release_dates
                    .iter()
                    .find(|d| d.certification.as_deref().is_some_and(|s| !s.is_empty()))
            })
            .and_then(|d| d.certification.clone());

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
                let mut cache = client.cache.lock().unwrap();
                if let Some(cached) = cache.get_search(&key) {
                    cache.record_search_hit();
                    return Ok(cached);
                }
                cache.record_search_miss();
            }
            let results = client
                .search_tmdb_movies(&query, year)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))?;
            let mut cache = client.cache.lock().unwrap();
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
                let mut cache = client.cache.lock().unwrap();
                if let Some(cached) = cache.get_search(&key) {
                    cache.record_search_hit();
                    return Ok(cached);
                }
                cache.record_search_miss();
            }
            let results = client
                .search_tmdb_tv(&query)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))?;
            let mut cache = client.cache.lock().unwrap();
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
                let mut cache = client.cache.lock().unwrap();
                if let Some(cached) = cache.get_search(&key) {
                    cache.record_search_hit();
                    return Ok(cached);
                }
                cache.record_search_miss();
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
            let mut cache = client.cache.lock().unwrap();
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
                let mut cache = client.cache.lock().unwrap();
                if let Some(cached) = cache.get_enrichment(&key) {
                    cache.record_enrichment_hit();
                    return Ok(cached);
                }
                cache.record_enrichment_miss();
            }
            let result = client
                .fetch_tmdb_enrichment(tmdb_id, &media_type)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))?;
            let mut cache = client.cache.lock().unwrap();
            cache.put_enrichment(key, result.clone());
            Ok(result)
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
        let results1 = client.search_movies("Fight Club", Some(1999)).await.unwrap();
        assert_eq!(results1.len(), 1);
        assert_eq!(results1[0].title, "Fight Club");

        // Second call — cache hit, should return same result without hitting server
        let results2 = client.search_movies("Fight Club", Some(1999)).await.unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].title, "Fight Club");

        // Verify cache stats
        let cache = client.cache.lock().unwrap();
        assert_eq!(cache.search_hits, 1);
        assert_eq!(cache.search_misses, 1);
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
        let results1 = client.search_movies("Fight Club", Some(1999)).await.unwrap();
        assert_eq!(results1.len(), 1);

        let results2 = client.search_movies("Fight Club", Some(2000)).await.unwrap();
        assert_eq!(results2.len(), 1);

        let cache = client.cache.lock().unwrap();
        assert_eq!(cache.search_hits, 0);
        assert_eq!(cache.search_misses, 2);
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

        // First call — cache miss
        let enrichment1 = client.fetch_tmdb_enrichment(550, &MediaType::Movie).await.unwrap();
        assert_eq!(enrichment1.runtime, Some(139));

        // Second call — cache hit
        let enrichment2 = client.fetch_tmdb_enrichment(550, &MediaType::Movie).await.unwrap();
        assert_eq!(enrichment2.runtime, Some(139));

        let cache = client.cache.lock().unwrap();
        assert_eq!(cache.enrichment_hits, 1);
        assert_eq!(cache.enrichment_misses, 1);
    }
}
