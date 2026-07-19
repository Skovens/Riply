use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the type of media content.
///
/// This enum distinguishes between movies and TV shows,
/// which affects how metadata is searched and how files are organized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    /// A movie (film).
    Movie,
    /// A TV show (series).
    Tv,
}

impl Default for MediaType {
    fn default() -> Self {
        MediaType::Movie
    }
}

impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaType::Movie => write!(f, "Movie"),
            MediaType::Tv => write!(f, "TV Show"),
        }
    }
}

impl MediaType {
    /// Check if this is a TV show.
    pub fn is_tv(&self) -> bool {
        matches!(self, MediaType::Tv)
    }

    /// Check if this is a movie.
    pub fn is_movie(&self) -> bool {
        matches!(self, MediaType::Movie)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        assert_eq!(MediaType::default(), MediaType::Movie);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", MediaType::Movie), "Movie");
        assert_eq!(format!("{}", MediaType::Tv), "TV Show");
    }

    #[test]
    fn test_is_tv() {
        assert!(!MediaType::Movie.is_tv());
        assert!(MediaType::Tv.is_tv());
    }

    #[test]
    fn test_is_movie() {
        assert!(MediaType::Movie.is_movie());
        assert!(!MediaType::Tv.is_movie());
    }

    #[test]
    fn test_serialization() {
        let movie = MediaType::Movie;
        let json = serde_json::to_string(&movie).unwrap();
        assert_eq!(json, "\"Movie\"");

        let tv = MediaType::Tv;
        let json = serde_json::to_string(&tv).unwrap();
        assert_eq!(json, "\"Tv\"");

        // Deserialization
        let movie: MediaType = serde_json::from_str("\"Movie\"").unwrap();
        assert_eq!(movie, MediaType::Movie);

        let tv: MediaType = serde_json::from_str("\"Tv\"").unwrap();
        assert_eq!(tv, MediaType::Tv);
    }
}