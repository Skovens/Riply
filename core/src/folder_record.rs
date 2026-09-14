use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

use crate::MediaType;

/// Represents the status of a folder in the Organizer pipeline.
///
/// This enum defines the state machine for folders as they progress
/// through scanning, analysis, review, and moving phases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderStatus {
    /// Folder has been scanned but not yet analyzed.
    Scanned,
    /// TMDb match found, year confirmed (confidence >= 0.9).
    Confident,
    /// TMDb match found, year missing or ambiguous (confidence 0.7–0.9).
    Uncertain,
    /// No TMDb match (confidence < 0.7).
    Unknown,
    /// Folder moved to library destination.
    Moved,
    /// Unknown folder moved to quarantine subfolder.
    Quarantined,
}

impl fmt::Display for FolderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FolderStatus::Scanned => write!(f, "Scanned"),
            FolderStatus::Confident => write!(f, "Confident"),
            FolderStatus::Uncertain => write!(f, "Uncertain"),
            FolderStatus::Unknown => write!(f, "Unknown"),
            FolderStatus::Moved => write!(f, "Moved"),
            FolderStatus::Quarantined => write!(f, "Quarantined"),
        }
    }
}

impl FolderStatus {
    /// Check if this status represents a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, FolderStatus::Moved | FolderStatus::Quarantined)
    }

    /// Check if this status requires user intervention.
    pub fn requires_review(&self) -> bool {
        matches!(self, FolderStatus::Uncertain | FolderStatus::Unknown)
    }

    /// Check if this status is confident enough to auto-move.
    pub fn is_confident(&self) -> bool {
        matches!(self, FolderStatus::Confident)
    }

    /// Check if this status is unknown (candidate for quarantine).
    pub fn is_unknown(&self) -> bool {
        matches!(self, FolderStatus::Unknown)
    }

    /// Check if this status indicates success.
    pub fn is_successful(&self) -> bool {
        matches!(self, FolderStatus::Confident | FolderStatus::Moved)
    }

    /// Check if this status is a pre-move state (ready to be moved).
    pub fn is_pre_move(&self) -> bool {
        matches!(self, FolderStatus::Confident | FolderStatus::Uncertain)
    }
}

/// Represents a scanned folder in the Organizer pipeline.
///
/// Each folder is treated as a single media unit (one movie or one TV season).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderRecord {
    /// Unique identifier (SHA-256 hash of folder path, first 16 hex chars).
    pub id: String,
    /// Full path to the folder on disk.
    pub path: PathBuf,
    /// Folder name.
    pub name: String,
    /// Suggested new name (from TMDb match).
    pub suggested_name: Option<String>,
    /// Suggested year (from TMDb match).
    pub suggested_year: Option<u32>,
    /// Confidence score (0.0 to 1.0).
    pub confidence: Option<f64>,
    /// TMDb ID for the media.
    pub tmdb_id: Option<i32>,
    /// Media type (movie or tv).
    pub media_type: MediaType,
    /// Number of files in the folder.
    pub file_count: usize,
    /// Biggest file in the folder (main title).
    pub main_file: Option<String>,
    /// Other files in the folder (extras).
    pub extra_files: Vec<String>,
    /// Current pipeline status.
    pub status: FolderStatus,
    /// Timestamp when the record was created.
    pub created_at: String,
    /// Timestamp when the record was last updated.
    pub updated_at: String,
}

impl FolderRecord {
    /// Create a new FolderRecord for a scanned folder.
    pub fn new(id: String, path: PathBuf, name: String, file_count: usize) -> Self {
        use chrono::Utc;
        let now = Utc::now().to_rfc3339();

        // Find the biggest file and collect extras
        let (main_file, extra_files) = if file_count > 0 {
            // This is a placeholder — actual file listing happens in OrganizerService
            (None, vec![])
        } else {
            (None, vec![])
        };

        FolderRecord {
            id,
            path,
            name,
            suggested_name: None,
            suggested_year: None,
            confidence: None,
            tmdb_id: None,
            media_type: MediaType::default(),
            file_count,
            main_file,
            extra_files,
            status: FolderStatus::Scanned,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Create a FolderRecord with full file listing (used after scan).
    pub fn with_files(
        id: String,
        path: PathBuf,
        name: String,
        files: Vec<(String, u64)>, // (filename, size)
    ) -> Self {
        use chrono::Utc;
        let now = Utc::now().to_rfc3339();

        let file_count = files.len();
        let (main_file, extra_files) = if files.is_empty() {
            (None, vec![])
        } else {
            let mut sorted = files;
            sorted.sort_by_key(|a| std::cmp::Reverse(a.1)); // Sort by size descending
            let main = sorted.first().map(|(name, _)| name.clone());
            let extras: Vec<String> = sorted.iter().skip(1).map(|(name, _)| name.clone()).collect();
            (main, extras)
        };

        FolderRecord {
            id,
            path,
            name,
            suggested_name: None,
            suggested_year: None,
            confidence: None,
            tmdb_id: None,
            media_type: MediaType::default(),
            file_count,
            main_file,
            extra_files,
            status: FolderStatus::Scanned,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Update the pipeline status.
    pub fn update_status(&mut self, new_status: FolderStatus) {
        use chrono::Utc;
        self.status = new_status;
        self.updated_at = Utc::now().to_rfc3339();
    }

    /// Apply a TMDb match result.
    pub fn apply_tmdb_match(
        &mut self,
        title: String,
        year: Option<u32>,
        tmdb_id: i32,
        media_type: MediaType,
    ) {
        use chrono::Utc;
        self.suggested_name = Some(title);
        self.suggested_year = year;
        self.tmdb_id = Some(tmdb_id);
        self.media_type = media_type;
        self.updated_at = Utc::now().to_rfc3339();

        // Determine confidence and status
        if year.is_some() {
            self.confidence = Some(0.95);
            self.status = FolderStatus::Confident;
        } else {
            self.confidence = Some(0.75);
            self.status = FolderStatus::Uncertain;
        }
    }

    /// Check if this folder needs user review.
    pub fn needs_review(&self) -> bool {
        self.status.requires_review()
    }

    /// Check if this folder is ready to be moved.
    pub fn is_ready_to_move(&self) -> bool {
        self.status.is_confident()
    }

    /// Check if this folder is unknown (candidate for quarantine).
    pub fn is_unknown(&self) -> bool {
        self.status.is_unknown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_folder_record() {
        let path = PathBuf::from("/test/Inception (2010)");
        let record = FolderRecord::new(
            "abc123".to_string(),
            path.clone(),
            "Inception (2010)".to_string(),
            5,
        );

        assert_eq!(record.id, "abc123");
        assert_eq!(record.path, path);
        assert_eq!(record.name, "Inception (2010)");
        assert_eq!(record.file_count, 5);
        assert_eq!(record.status, FolderStatus::Scanned);
        assert_eq!(record.media_type, MediaType::Movie);
        assert!(record.created_at.contains("T"));
        assert_eq!(record.created_at, record.updated_at);
        assert!(record.main_file.is_none());
        assert!(record.extra_files.is_empty());
    }

    #[test]
    fn test_folder_record_with_files() {
        let path = PathBuf::from("/test/Inception (2010)");
        let files = vec![
            ("Inception.mkv".to_string(), 8000000000), // 8GB
            ("Commentary.mkv".to_string(), 500000000), // 500MB
            ("Behind the Scenes.mkv".to_string(), 300000000), // 300MB
        ];
        let record = FolderRecord::with_files(
            "def456".to_string(),
            path.clone(),
            "Inception (2010)".to_string(),
            files,
        );

        assert_eq!(record.file_count, 3);
        assert_eq!(record.main_file, Some("Inception.mkv".to_string()));
        assert_eq!(record.extra_files, vec!["Commentary.mkv".to_string(), "Behind the Scenes.mkv".to_string()]);
        assert_eq!(record.status, FolderStatus::Scanned);
    }

    #[test]
    fn test_apply_tmdb_match_with_year() {
        let mut record = FolderRecord::new(
            "ghi789".to_string(),
            PathBuf::from("/test/Disc Name"),
            "Disc Name".to_string(),
            2,
        );

        record.apply_tmdb_match(
            "Inception".to_string(),
            Some(2010),
            27205,
            MediaType::Movie,
        );

        assert_eq!(record.suggested_name, Some("Inception".to_string()));
        assert_eq!(record.suggested_year, Some(2010));
        assert_eq!(record.tmdb_id, Some(27205));
        assert_eq!(record.media_type, MediaType::Movie);
        assert_eq!(record.confidence, Some(0.95));
        assert_eq!(record.status, FolderStatus::Confident);
    }

    #[test]
    fn test_apply_tmdb_match_without_year() {
        let mut record = FolderRecord::new(
            "jkl012".to_string(),
            PathBuf::from("/test/Disc Name"),
            "Disc Name".to_string(),
            3,
        );

        record.apply_tmdb_match(
            "Doctor Who".to_string(),
            None,
            57247,
            MediaType::Tv,
        );

        assert_eq!(record.suggested_name, Some("Doctor Who".to_string()));
        assert_eq!(record.suggested_year, None);
        assert_eq!(record.tmdb_id, Some(57247));
        assert_eq!(record.media_type, MediaType::Tv);
        assert_eq!(record.confidence, Some(0.75));
        assert_eq!(record.status, FolderStatus::Uncertain);
    }

    #[test]
    fn test_update_status() {
        let mut record = FolderRecord::new(
            "mno345".to_string(),
            PathBuf::from("/test/Folder"),
            "Folder".to_string(),
            1,
        );

        let original_updated = record.updated_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(10));

        record.update_status(FolderStatus::Confident);
        assert_eq!(record.status, FolderStatus::Confident);
        assert!(record.updated_at > original_updated);
    }

    #[test]
    fn test_needs_review() {
        let mut record = FolderRecord::new(
            "pqr678".to_string(),
            PathBuf::from("/test/Folder"),
            "Folder".to_string(),
            1,
        );

        // Scanned doesn't need review
        assert!(!record.needs_review());

        // Confident doesn't need review
        record.update_status(FolderStatus::Confident);
        assert!(!record.needs_review());

        // Uncertain needs review
        record.update_status(FolderStatus::Uncertain);
        assert!(record.needs_review());

        // Unknown needs review
        record.update_status(FolderStatus::Unknown);
        assert!(record.needs_review());

        // Moved doesn't need review
        record.update_status(FolderStatus::Moved);
        assert!(!record.needs_review());
    }

    #[test]
    fn test_is_ready_to_move() {
        let mut record = FolderRecord::new(
            "stu901".to_string(),
            PathBuf::from("/test/Folder"),
            "Folder".to_string(),
            1,
        );

        assert!(!record.is_ready_to_move());

        record.update_status(FolderStatus::Confident);
        assert!(record.is_ready_to_move());

        record.update_status(FolderStatus::Uncertain);
        assert!(!record.is_ready_to_move());
    }

    #[test]
    fn test_folder_status_methods() {
        assert!(FolderStatus::Moved.is_terminal());
        assert!(FolderStatus::Quarantined.is_terminal());
        assert!(!FolderStatus::Scanned.is_terminal());
        assert!(!FolderStatus::Confident.is_terminal());

        assert!(FolderStatus::Uncertain.requires_review());
        assert!(FolderStatus::Unknown.requires_review());
        assert!(!FolderStatus::Confident.requires_review());

        assert!(FolderStatus::Confident.is_confident());
        assert!(!FolderStatus::Uncertain.is_confident());

        assert!(FolderStatus::Unknown.is_unknown());
        assert!(!FolderStatus::Uncertain.is_unknown());

        assert!(FolderStatus::Confident.is_successful());
        assert!(FolderStatus::Moved.is_successful());
        assert!(!FolderStatus::Scanned.is_successful());

        assert!(FolderStatus::Confident.is_pre_move());
        assert!(FolderStatus::Uncertain.is_pre_move());
        assert!(!FolderStatus::Unknown.is_pre_move());
    }

    #[test]
    fn test_folder_status_display() {
        assert_eq!(format!("{}", FolderStatus::Scanned), "Scanned");
        assert_eq!(format!("{}", FolderStatus::Confident), "Confident");
        assert_eq!(format!("{}", FolderStatus::Uncertain), "Uncertain");
        assert_eq!(format!("{}", FolderStatus::Unknown), "Unknown");
        assert_eq!(format!("{}", FolderStatus::Moved), "Moved");
        assert_eq!(format!("{}", FolderStatus::Quarantined), "Quarantined");
    }

    #[test]
    fn test_serialization() {
        let record = FolderRecord::new(
            "ser123".to_string(),
            PathBuf::from("/test/Folder"),
            "Folder".to_string(),
            2,
        );
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"id\":\"ser123\""));
        assert!(json.contains("\"name\":\"Folder\""));
        assert!(json.contains("\"status\":\"Scanned\""));

        let deserialized: FolderRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "ser123");
        assert_eq!(deserialized.name, "Folder");
        assert_eq!(deserialized.status, FolderStatus::Scanned);
    }
}
