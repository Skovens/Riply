use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{MediaType, PipelineStatus};

/// Whether a file is the main title or an extra (bonus feature).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ContentType {
    /// Main feature (typically > 1 hour runtime).
    Main,
    /// Bonus content (trailers, behind-the-scenes, commentary, etc.).
    Extra,
    /// Not yet classified.
    #[default]
    Unknown,
}

impl ContentType {
    /// Check if this is an extra/bonus feature.
    pub fn is_extra(&self) -> bool {
        matches!(self, ContentType::Extra)
    }

    /// Parse from a string value (for deserialization).
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "extra" | "bonus" => ContentType::Extra,
            "main" | "title" => ContentType::Main,
            _ => ContentType::Unknown,
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Main => write!(f, "main"),
            ContentType::Extra => write!(f, "extra"),
            ContentType::Unknown => write!(f, "unknown"),
        }
    }
}

/// Convert ContentType to a string for database storage.
pub fn content_type_to_str(ct: &ContentType) -> &str {
    match ct {
        ContentType::Main => "main",
        ContentType::Extra => "extra",
        ContentType::Unknown => "unknown",
    }
}

// ── Split FileRecord into domain-focused sub-records ──────────────────────────

/// Core scanning data: file identity, location, and pipeline state.
///
/// Populated first by the scanner, then gradually enriched by analysis
/// and enrichment stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRecord {
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
    /// Whether this is the main feature or a bonus/extras file.
    pub content_type: ContentType,
    /// Media type (movie or tv).
    pub media_type: MediaType,
    /// ID of the TitleGroup this file belongs to, if any.
    pub title_group_id: Option<String>,
    /// Path to a duplicate file (if detected).
    pub duplicate_path: Option<PathBuf>,
    /// Whether the file has been reviewed.
    pub reviewed: bool,
    /// Timestamp when the record was created.
    pub created_at: String,
    /// Timestamp when the record was last updated.
    pub updated_at: String,
}

/// Title and episode metadata from search or manual review.
///
/// Populated by the analyzer (TMDb/OMDb lookups) and the review step.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataRecord {
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
}

/// Enrichment data from metadata providers (genres, cast, etc.).
///
/// Populated after a match is confirmed, via TMDb/OMb enrichment calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnrichmentRecord {
    /// Subtitle text extracted via OCR.
    pub subtitle_text: Option<String>,
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
}

/// Represents a scanned media file in the pipeline.
///
/// Composed of three focused sub-records:
/// - `scan`: file identity and pipeline state (always populated)
/// - `metadata`: title/episode info (populated by analysis/review)
/// - `enrichment`: genres/cast/runtime (populated after match)
///
/// Convenience accessors delegate to the sub-records so existing code
/// (`file.filename`, `file.canonical_title`, etc.) continues to work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub scan: ScanRecord,
    pub metadata: MetadataRecord,
    pub enrichment: EnrichmentRecord,
}

// ── FileRecord convenience accessors (delegate to sub-records) ────────────────

impl FileRecord {
    // ── ScanRecord accessors ──────────────────────────────────────────────
    pub fn id(&self) -> &str { &self.scan.id }
    pub fn path(&self) -> &PathBuf { &self.scan.path }
    pub fn filename(&self) -> &str { &self.scan.filename }
    pub fn size(&self) -> Option<i64> { self.scan.size }
    pub fn duration(&self) -> Option<i64> { self.scan.duration }
    pub fn hash(&self) -> &str { &self.scan.hash }
    pub fn status(&self) -> &PipelineStatus { &self.scan.status }
    pub fn content_type(&self) -> &ContentType { &self.scan.content_type }
    pub fn media_type(&self) -> &MediaType { &self.scan.media_type }
    pub fn title_group_id(&self) -> &Option<String> { &self.scan.title_group_id }
    pub fn duplicate_path(&self) -> &Option<PathBuf> { &self.scan.duplicate_path }
    pub fn reviewed(&self) -> bool { self.scan.reviewed }
    pub fn created_at(&self) -> &str { &self.scan.created_at }
    pub fn updated_at(&self) -> &str { &self.scan.updated_at }

    // ── MetadataRecord accessors ──────────────────────────────────────────
    pub fn suggested_title(&self) -> &Option<String> { &self.metadata.suggested_title }
    pub fn suggested_year(&self) -> &Option<u32> { &self.metadata.suggested_year }
    pub fn confidence(&self) -> &Option<f64> { &self.metadata.confidence }
    pub fn canonical_title(&self) -> &Option<String> { &self.metadata.canonical_title }
    pub fn canonical_year(&self) -> &Option<u32> { &self.metadata.canonical_year }
    pub fn tmdb_id(&self) -> &Option<i32> { &self.metadata.tmdb_id }
    pub fn series_name(&self) -> &Option<String> { &self.metadata.series_name }
    pub fn season_number(&self) -> &Option<u32> { &self.metadata.season_number }
    pub fn episode_number(&self) -> &Option<u32> { &self.metadata.episode_number }
    pub fn episode_title(&self) -> &Option<String> { &self.metadata.episode_title }
    pub fn source(&self) -> &Option<String> { &self.metadata.source }

    // ── EnrichmentRecord accessors ────────────────────────────────────────
    pub fn subtitle_text(&self) -> &Option<String> { &self.enrichment.subtitle_text }
    pub fn genres(&self) -> &Option<Vec<String>> { &self.enrichment.genres }
    pub fn runtime(&self) -> &Option<i32> { &self.enrichment.runtime }
    pub fn content_rating(&self) -> &Option<String> { &self.enrichment.content_rating }
    pub fn overview(&self) -> &Option<String> { &self.enrichment.overview }
    pub fn cast_list(&self) -> &Option<Vec<String>> { &self.enrichment.cast_list }

    // ── Mutators ──────────────────────────────────────────────────────────
    pub fn id_mut(&mut self) -> &mut String { &mut self.scan.id }
    pub fn path_mut(&mut self) -> &mut PathBuf { &mut self.scan.path }
    pub fn filename_mut(&mut self) -> &mut String { &mut self.scan.filename }
    pub fn size_mut(&mut self) -> &mut Option<i64> { &mut self.scan.size }
    pub fn duration_mut(&mut self) -> &mut Option<i64> { &mut self.scan.duration }
    pub fn hash_mut(&mut self) -> &mut String { &mut self.scan.hash }
    pub fn status_mut(&mut self) -> &mut PipelineStatus { &mut self.scan.status }
    pub fn content_type_mut(&mut self) -> &mut ContentType { &mut self.scan.content_type }
    pub fn media_type_mut(&mut self) -> &mut MediaType { &mut self.scan.media_type }
    pub fn title_group_id_mut(&mut self) -> &mut Option<String> { &mut self.scan.title_group_id }
    pub fn duplicate_path_mut(&mut self) -> &mut Option<PathBuf> { &mut self.scan.duplicate_path }
    pub fn reviewed_mut(&mut self) -> &mut bool { &mut self.scan.reviewed }
    pub fn created_at_mut(&mut self) -> &mut String { &mut self.scan.created_at }
    pub fn updated_at_mut(&mut self) -> &mut String { &mut self.scan.updated_at }

    pub fn suggested_title_mut(&mut self) -> &mut Option<String> { &mut self.metadata.suggested_title }
    pub fn suggested_year_mut(&mut self) -> &mut Option<u32> { &mut self.metadata.suggested_year }
    pub fn confidence_mut(&mut self) -> &mut Option<f64> { &mut self.metadata.confidence }
    pub fn canonical_title_mut(&mut self) -> &mut Option<String> { &mut self.metadata.canonical_title }
    pub fn canonical_year_mut(&mut self) -> &mut Option<u32> { &mut self.metadata.canonical_year }
    pub fn tmdb_id_mut(&mut self) -> &mut Option<i32> { &mut self.metadata.tmdb_id }
    pub fn series_name_mut(&mut self) -> &mut Option<String> { &mut self.metadata.series_name }
    pub fn season_number_mut(&mut self) -> &mut Option<u32> { &mut self.metadata.season_number }
    pub fn episode_number_mut(&mut self) -> &mut Option<u32> { &mut self.metadata.episode_number }
    pub fn episode_title_mut(&mut self) -> &mut Option<String> { &mut self.metadata.episode_title }
    pub fn source_mut(&mut self) -> &mut Option<String> { &mut self.metadata.source }

    pub fn subtitle_text_mut(&mut self) -> &mut Option<String> { &mut self.enrichment.subtitle_text }
    pub fn genres_mut(&mut self) -> &mut Option<Vec<String>> { &mut self.enrichment.genres }
    pub fn runtime_mut(&mut self) -> &mut Option<i32> { &mut self.enrichment.runtime }
    pub fn content_rating_mut(&mut self) -> &mut Option<String> { &mut self.enrichment.content_rating }
    pub fn overview_mut(&mut self) -> &mut Option<String> { &mut self.enrichment.overview }
    pub fn cast_list_mut(&mut self) -> &mut Option<Vec<String>> { &mut self.enrichment.cast_list }

    // ── Factory ───────────────────────────────────────────────────────────
    /// Create a new FileRecord for a scanned file.
    pub fn new(id: String, path: PathBuf, filename: String, hash: String) -> Self {
        use chrono::Utc;
        let now = Utc::now().to_rfc3339();

        FileRecord {
            scan: ScanRecord {
                id,
                path,
                filename,
                size: None,
                duration: None,
                hash,
                status: PipelineStatus::Scanned,
                content_type: ContentType::Unknown,
                media_type: MediaType::default(),
                title_group_id: None,
                duplicate_path: None,
                reviewed: false,
                created_at: now.clone(),
                updated_at: now,
            },
            metadata: MetadataRecord::default(),
            enrichment: EnrichmentRecord::default(),
        }
    }

    /// Update the pipeline status.
    pub fn update_status(&mut self, new_status: PipelineStatus) {
        use chrono::Utc;
        self.scan.status = new_status;
        self.scan.updated_at = Utc::now().to_rfc3339();
    }

    /// Check if this file is a TV show episode.
    pub fn is_tv_episode(&self) -> bool {
        self.scan.media_type.is_tv() && self.metadata.episode_number.is_some()
    }

    /// Check if this file is marked as an extra.
    pub fn is_extra(&self) -> bool {
        self.scan.content_type.is_extra()
    }

    /// Check if this file needs review.
    pub fn needs_review(&self) -> bool {
        self.scan.status.requires_review()
    }

    /// Check if this file is ready to be moved.
    pub fn is_ready_to_move(&self) -> bool {
        matches!(self.scan.status, PipelineStatus::Confirmed)
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

        assert_eq!(record.id(), "abc123");
        assert_eq!(record.path(), &path);
        assert_eq!(record.filename(), "movie.mkv");
        assert_eq!(record.hash(), "hash123");
        assert_eq!(*record.status(), PipelineStatus::Scanned);
        assert_eq!(*record.media_type(), MediaType::Movie);
        assert_eq!(*record.content_type(), ContentType::Unknown);
        assert!(!record.reviewed());
        assert!(record.created_at().contains("T"));
        assert_eq!(record.created_at(), record.updated_at());
    }

    #[test]
    fn test_update_status() {
        let mut record = FileRecord::new(
            "abc123".to_string(),
            PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        let original_updated = record.updated_at().to_string();
        std::thread::sleep(std::time::Duration::from_millis(10));

        record.update_status(PipelineStatus::Matched);
        assert_eq!(*record.status(), PipelineStatus::Matched);
        assert!(record.updated_at() > &original_updated);
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
        *record.media_type_mut() = MediaType::Tv;
        assert!(!record.is_tv_episode());

        // TV show with episode number
        *record.episode_number_mut() = Some(1);
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

        *record.content_type_mut() = ContentType::Extra;
        assert!(record.is_extra());

        *record.content_type_mut() = ContentType::Main;
        assert!(!record.is_extra());
    }

    #[test]
    fn test_content_type_from_str() {
        assert_eq!(ContentType::from_str("extra"), ContentType::Extra);
        assert_eq!(ContentType::from_str("EXTRA"), ContentType::Extra);
        assert_eq!(ContentType::from_str("bonus"), ContentType::Extra);
        assert_eq!(ContentType::from_str("main"), ContentType::Main);
        assert_eq!(ContentType::from_str("MAIN"), ContentType::Main);
        assert_eq!(ContentType::from_str("title"), ContentType::Main);
        assert_eq!(ContentType::from_str("unknown"), ContentType::Unknown);
        assert_eq!(ContentType::from_str("foo"), ContentType::Unknown);
    }

    #[test]
    fn test_content_type_display() {
        assert_eq!(ContentType::Main.to_string(), "main");
        assert_eq!(ContentType::Extra.to_string(), "extra");
        assert_eq!(ContentType::Unknown.to_string(), "unknown");
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

    #[test]
    fn test_record_split_accessors() {
        let mut record = FileRecord::new(
            "split-test".to_string(),
            PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash-split".to_string(),
        );

        // ScanRecord fields
        assert_eq!(record.id(), "split-test");
        assert_eq!(record.filename(), "movie.mkv");
        assert_eq!(*record.status(), PipelineStatus::Scanned);

        // Set metadata fields via mutators
        *record.canonical_title_mut() = Some("Test Movie".to_string());
        assert_eq!(*record.canonical_title(), Some("Test Movie".to_string()));

        // Set enrichment fields via mutators
        *record.genres_mut() = Some(vec!["Drama".to_string()]);
        assert_eq!(*record.genres(), Some(vec!["Drama".to_string()]));
    }
}
