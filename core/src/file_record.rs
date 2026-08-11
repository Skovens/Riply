use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{MediaType, PipelineStatus};

/// Represents a scanned media file in the pipeline.
///
/// This struct holds all metadata for a file as it progresses through
/// the scanning, analysis, and review phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    /// Unique identifier (SHA-256 hash of file content).
    pub id: String,
    /// Full path to the file on disk.
    pub path: PathBuf,
    /// File name (without path).
    pub filename: String,
    /// File size in bytes.
    pub size: Option<i64>,
    /// Duration in seconds (from ffprobe).
    pub duration: Option<i64>,
    /// File hash for identification.
    pub hash: String,
    /// Current pipeline status.
    pub status: PipelineStatus,
    /// Suggested title from metadata search.
    pub suggested_title: Option<String>,
    /// Suggested year from metadata search.
    pub suggested_year: Option<u32>,
    /// Confidence score (0.0 to 1.0).
    pub confidence: Option<f64>,
    /// Canonical title (confirmed title).
    pub canonical_title: Option<String>,
    /// Canonical year (confirmed year).
    pub canonical_year: Option<u32>,
    /// TMDb ID for the media.
    pub tmdb_id: Option<i32>,
    /// Content type (main or extra).
    pub content_type: String,
    /// Media type (movie or tv).
    pub media_type: MediaType,
    /// Series name (for TV shows).
    pub series_name: Option<String>,
    /// Season number (for TV shows).
    pub season_number: Option<u32>,
    /// Episode number (for TV shows).
    pub episode_number: Option<u32>,
    /// Episode title (for TV shows).
    pub episode_title: Option<String>,
    /// Source of the metadata (TMDb, OMDb, Manual).
    pub source: Option<String>,
    /// Subtitle text extracted via OCR.
    pub subtitle_text: Option<String>,
    /// Path to a duplicate file (if detected).
    pub duplicate_path: Option<PathBuf>,
    /// ID of the TitleGroup this file belongs to, if any.
    pub title_group_id: Option<String>,
    /// Genres from metadata enrichment (e.g. ["Drama", "Crime"]).
    pub genres: Option<Vec<String>>,
    /// Runtime in minutes from metadata enrichment.
    pub runtime: Option<i32>,
    /// Content rating (e.g. "PG-13", "TV-MA").
    pub content_rating: Option<String>,
    /// Plot overview from metadata enrichment.
    pub overview: Option<String>,
    /// Leading cast member names from metadata enrichment.
    pub cast_list: Option<Vec<String>>,
    /// Whether the file has been reviewed.
    pub reviewed: bool,
    /// Timestamp when the record was created.
    pub created_at: String,
    /// Timestamp when the record was last updated.
    pub updated_at: String,
}

impl FileRecord {
    /// Create a new FileRecord for a scanned file.
    pub fn new(id: String, path: PathBuf, filename: String, hash: String) -> Self {
        use chrono::Utc;
        let now = Utc::now().to_rfc3339();

        FileRecord {
            id,
            path,
            filename,
            size: None,
            duration: None,
            hash,
            status: PipelineStatus::Scanned,
            suggested_title: None,
            suggested_year: None,
            confidence: None,
            canonical_title: None,
            canonical_year: None,
            tmdb_id: None,
            content_type: "unknown".to_string(),
            media_type: MediaType::default(),
            series_name: None,
            season_number: None,
            episode_number: None,
            episode_title: None,
            source: None,
            subtitle_text: None,
            duplicate_path: None,
            title_group_id: None,
            genres: None,
            runtime: None,
            content_rating: None,
            overview: None,
            cast_list: None,
            reviewed: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Update the pipeline status.
    pub fn update_status(&mut self, new_status: PipelineStatus) {
        use chrono::Utc;
        self.status = new_status;
        self.updated_at = Utc::now().to_rfc3339();
    }

    /// Check if this file is a TV show episode.
    pub fn is_tv_episode(&self) -> bool {
        self.media_type.is_tv() && self.episode_number.is_some()
    }

    /// Check if this file is marked as an extra.
    pub fn is_extra(&self) -> bool {
        self.content_type == "extra"
    }

    /// Check if this file needs review.
    pub fn needs_review(&self) -> bool {
        self.status.requires_review()
    }

    /// Check if this file is ready to be moved.
    pub fn is_ready_to_move(&self) -> bool {
        matches!(self.status, PipelineStatus::Confirmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PipelineStatus;

    #[test]
    fn test_new_file_record() {
        let path = PathBuf::from("/test/movie.mkv");
        let record = FileRecord::new(
            "abc123".to_string(),
            path.clone(),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        assert_eq!(record.id, "abc123");
        assert_eq!(record.path, path);
        assert_eq!(record.filename, "movie.mkv");
        assert_eq!(record.hash, "hash123");
        assert_eq!(record.status, PipelineStatus::Scanned);
        assert_eq!(record.media_type, MediaType::Movie);
        assert_eq!(record.content_type, "unknown");
        assert!(!record.reviewed);
        assert!(record.created_at.contains("T"));
        assert_eq!(record.created_at, record.updated_at);
    }

    #[test]
    fn test_update_status() {
        let mut record = FileRecord::new(
            "abc123".to_string(),
            PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        let original_updated = record.updated_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(10));

        record.update_status(PipelineStatus::Matched);
        assert_eq!(record.status, PipelineStatus::Matched);
        assert!(record.updated_at > original_updated);
    }

    #[test]
    fn test_is_tv_episode() {
        let mut record = FileRecord::new(
            "abc123".to_string(),
            PathBuf::from("/test/show.mkv"),
            "show.mkv".to_string(),
            "hash123".to_string(),
        );

        // Not a TV show
        assert!(!record.is_tv_episode());

        // TV show but no episode number
        record.media_type = MediaType::Tv;
        assert!(!record.is_tv_episode());

        // TV show with episode number
        record.episode_number = Some(1);
        assert!(record.is_tv_episode());
    }

    #[test]
    fn test_is_extra() {
        let mut record = FileRecord::new(
            "abc123".to_string(),
            PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        assert!(!record.is_extra());

        record.content_type = "extra".to_string();
        assert!(record.is_extra());
    }

    #[test]
    fn test_needs_review() {
        let mut record = FileRecord::new(
            "abc123".to_string(),
            PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        // Scanned files don't need review
        assert!(!record.needs_review());

        // Needs review status
        record.update_status(PipelineStatus::NeedsReview);
        assert!(record.needs_review());

        // Matched files don't need review
        record.update_status(PipelineStatus::Matched);
        assert!(!record.needs_review());
    }

    #[test]
    fn test_is_ready_to_move() {
        let mut record = FileRecord::new(
            "abc123".to_string(),
            PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        // Scanned files are not ready to move
        assert!(!record.is_ready_to_move());

        // Matched files are not ready to move
        record.update_status(PipelineStatus::Matched);
        assert!(!record.is_ready_to_move());

        // Confirmed files are ready to move
        record.update_status(PipelineStatus::Confirmed);
        assert!(record.is_ready_to_move());

        // Moved files are not ready to move (already moved)
        record.update_status(PipelineStatus::Moved);
        assert!(!record.is_ready_to_move());
    }
}
