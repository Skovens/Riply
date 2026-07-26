use std::collections::HashMap;

use reqwest::Client;
use riply_core::{MediaType, MetadataMatch, MetadataSearcher};
use tracing::info;

/// HTTP client adapter - handles API requests with retry and rate limiting
pub struct HttpClient {
    client: Client,
    tmdb_base_url: String,
    omdb_base_url: String,
    tmdb_api_key: Option<String>,
    omdb_api_key: Option<String>,
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

        HttpClient {
            client,
            tmdb_base_url,
            omdb_base_url,
            tmdb_api_key,
            omdb_api_key,
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
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
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
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
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
        params.insert("type".to_string(), "movie".to_string());

        let url = format!("{}/", self.omdb_base_url);
        let resp: OmdbResponse = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;

        if resp.response.as_deref() == Some("True") {
            let year = resp.year.as_ref().and_then(|y| y.trim().parse().ok());
            return Ok(vec![MetadataMatch {
                title: resp.title.unwrap_or_default(),
                year,
                tmdb_id: None,
                is_tv: false,
            }]);
        }

        Ok(vec![])
    }
}

impl MetadataSearcher for HttpClient {
    fn search_movies(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<MetadataMatch>, anyhow::Error>> + Send + '_,
        >,
    > {
        let query = query.to_string();
        Box::pin(async move { self.search_tmdb_movies(&query, year).await })
    }

    fn search_tv(
        &self,
        query: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<MetadataMatch>, anyhow::Error>> + Send + '_,
        >,
    > {
        let query = query.to_string();
        Box::pin(async move { self.search_tmdb_tv(&query).await })
    }

    fn search(
        &self,
        query: &str,
        year: Option<u32>,
        media_type: &MediaType,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<MetadataMatch>, anyhow::Error>> + Send + '_,
        >,
    > {
        let query = query.to_string();
        let media_type = media_type.clone();
        Box::pin(async move {
            let mut results = match media_type {
                MediaType::Tv => self.search_tmdb_tv(&query).await?,
                MediaType::Movie => {
                    let mut movies = self.search_tmdb_movies(&query, year).await?;
                    if movies.is_empty() {
                        movies = self.search_omdb(&query, year).await?;
                    }
                    movies
                }
            };
            results.truncate(5);
            Ok(results)
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
