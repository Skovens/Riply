use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::MediaType;

/// Status of a TitleGroup as it moves through the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TitleGroupStatus {
    /// Newly scanned, no metadata lookup done yet.
    New,
    /// Metadata has been matched (title, year, etc. are filled).
    Matched,
    /// User has confirmed the group is correct.
    Confirmed,
    /// All files in the group have been moved to their final destination.
    Moved,
}

impl std::fmt::Display for TitleGroupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TitleGroupStatus::New => write!(f, "New"),
            TitleGroupStatus::Matched => write!(f, "Matched"),
            TitleGroupStatus::Confirmed => write!(f, "Confirmed"),
            TitleGroupStatus::Moved => write!(f, "Moved"),
        }
    }
}

/// A group of files that belong to the same title, typically from a single
/// rip session stored in a subdirectory of the staging area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleGroup {
    /// Unique identifier (hash of source_directory).
    pub id: String,
    /// The subdirectory containing the files.
    pub source_directory: PathBuf,
    /// Matched or cleaned title for the group.
    pub title: String,
    /// Matched year, if any.
    pub year: Option<u16>,
    /// TMDb ID, if matched.
    pub tmdb_id: Option<i32>,
    /// Media type of the group.
    pub media_type: MediaType,
    /// Series name (for TV shows).
    pub series_name: Option<String>,
    /// Current status of the group.
    pub status: TitleGroupStatus,
}

impl TitleGroup {
    /// Create a new TitleGroup with the given ID and source directory.
    pub fn new(id: String, source_directory: PathBuf, title: String) -> Self {
        TitleGroup {
            id,
            source_directory,
            title,
            year: None,
            tmdb_id: None,
            media_type: MediaType::Movie,
            series_name: None,
            status: TitleGroupStatus::New,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_group_new() {
        let dir = PathBuf::from("/staging/Inception (2010)");
        let group = TitleGroup::new(
            "abc123".to_string(),
            dir.clone(),
            "Inception".to_string(),
        );
        assert_eq!(group.id, "abc123");
        assert_eq!(group.source_directory, dir);
        assert_eq!(group.title, "Inception");
        assert_eq!(group.media_type, MediaType::Movie);
        assert_eq!(group.status, TitleGroupStatus::New);
    }

    #[test]
    fn test_title_group_status_display() {
        assert_eq!(TitleGroupStatus::New.to_string(), "New");
        assert_eq!(TitleGroupStatus::Matched.to_string(), "Matched");
        assert_eq!(TitleGroupStatus::Confirmed.to_string(), "Confirmed");
        assert_eq!(TitleGroupStatus::Moved.to_string(), "Moved");
    }
}
