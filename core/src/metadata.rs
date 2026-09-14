use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::MediaType;

/// Metadata found from a search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataMatch {
    pub title: String,
    pub year: Option<u32>,
    pub tmdb_id: Option<i32>,
    pub is_tv: bool,
    /// Series name (for TV shows, may differ from search query)
    pub series_name: Option<String>,
    /// Season number (if known from filename or API)
    pub season_number: Option<u32>,
    /// Episode number (if known from filename or API)
    pub episode_number: Option<u32>,
    /// Episode title (from TMDb episode details)
    pub episode_title: Option<String>,
}

/// Additional metadata fetched after a match is confirmed (genres, runtime,
/// content rating, overview, cast). Populated during analysis so the review
/// page and move step have richer information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataEnrichment {
    /// Genres as returned by the provider (e.g. ["Drama", "Crime"]).
    pub genres: Option<Vec<String>>,
    /// Runtime in minutes.
    pub runtime: Option<i32>,
    /// Content rating (e.g. "PG-13", "TV-MA").
    pub content_rating: Option<String>,
    /// Plot overview.
    pub overview: Option<String>,
    /// Leading cast member names.
    pub cast_list: Option<Vec<String>>,
}

/// Errors that can occur during metadata search operations.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Rate limited")]
    RateLimited,
}

/// Port for searching external metadata APIs (TMDb, OMDb, etc.)
///
/// Implemented by infra adapters, consumed by app services.
pub trait MetadataSearcher: Send + Sync {
    fn search_movies(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_>>;

    fn search_tv(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_>>;

    fn search(
        &self,
        query: &str,
        year: Option<u32>,
        media_type: &MediaType,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, MetadataError>> + Send + '_>> {
        let media_type = media_type.clone();
        let query = query.to_string();
        Box::pin(async move {
            match media_type {
                MediaType::Tv => self.search_tv(&query).await,
                MediaType::Movie => self.search_movies(&query, year).await,
            }
        })
    }

    /// Fetch enrichment metadata (genres, runtime, rating, overview, cast) for
    /// a matched title. Defaults to an empty result so mocks and providers that
    /// don't support enrichment can ignore it.
    fn fetch_enrichment(
        &self,
        _tmdb_id: i32,
        _media_type: &MediaType,
    ) -> Pin<Box<dyn Future<Output = Result<MetadataEnrichment, MetadataError>> + Send + '_>> {
        Box::pin(async { Ok(MetadataEnrichment::default()) })
    }

    /// Log cache and usage statistics. Default is a no-op so mocks don't need
    /// to implement it.
    fn log_stats(&self) {}
}
