use std::collections::HashMap;

use backon::{ExponentialBuilder, Retryable};
use reqwest::Client;
use riply_core::{MediaType, MetadataEnrichment, MetadataError, MetadataMatch, MetadataSearcher};
use tracing::info;

use crate::rate_limiter::RateLimiter;

/// HTTP client adapter - handles API requests with retry and rate limiting
pub struct HttpClient {
    client: Client,
    tmdb_base_url: String,
    omdb_base_url: String,
    tmdb_api_key: Option<String>,
    omdb_api_key: Option<String>,
    tmdb_limiter: Option<RateLimiter>,
    omdb_limiter: Option<RateLimiter>,
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

        let tmdb_limiter = tmdb_api_key.is_some().then(|| RateLimiter::new(40, 4.0));
        let omdb_limiter = omdb_api_key.is_some().then(|| RateLimiter::new(10, 10.0));

        HttpClient {
            client,
            tmdb_base_url,
            omdb_base_url,
            tmdb_api_key,
            omdb_api_key,
            tmdb_limiter,
            omdb_limiter,
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

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        params: &HashMap<String, String>,
        limiter: Option<&RateLimiter>,
    ) -> Result<T, anyhow::Error> {
        if let Some(limiter) = limiter {
            limiter.acquire().await;
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
        .retry(ExponentialBuilder::default())
        .await?;
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
        let resp: TmdbSearchResponse = self
            .get_json(&url, &params, self.tmdb_limiter.as_ref())
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
            .get_json(&url, &params, self.tmdb_limiter.as_ref())
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
            .get_json(&url, &params, self.omdb_limiter.as_ref())
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
            .get_json(&url, &params, self.omdb_limiter.as_ref())
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
            .get_json(&url, &params, self.tmdb_limiter.as_ref())
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
        Box::pin(async move {
            self.search_tmdb_movies(&query, year)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))
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
        Box::pin(async move {
            self.search_tmdb_tv(&query)
                .await
                .map_err(|e| MetadataError::ApiError(e.to_string()))
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
        Box::pin(async move {
            let mut results = match media_type {
                MediaType::Tv => {
                    let mut tv = self
                        .search_tmdb_tv(&query)
                        .await
                        .map_err(|e| MetadataError::ApiError(e.to_string()))?;
                    if tv.is_empty() {
                        tv = self
                            .search_omdb_tv(&query, year)
                            .await
                            .map_err(|e| MetadataError::ApiError(e.to_string()))?;
                    }
                    tv
                }
                MediaType::Movie => {
                    let mut movies = self
                        .search_tmdb_movies(&query, year)
                        .await
                        .map_err(|e| MetadataError::ApiError(e.to_string()))?;
                    if movies.is_empty() {
                        movies = self
                            .search_omdb(&query, year)
                            .await
                            .map_err(|e| MetadataError::ApiError(e.to_string()))?;
                    }
                    movies
                }
            };
            results.truncate(5);
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
        Box::pin(async move {
            self.fetch_tmdb_enrichment(tmdb_id, &media_type)
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
}
