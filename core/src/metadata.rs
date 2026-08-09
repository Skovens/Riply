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
}
