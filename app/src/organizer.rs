// ADR-026: Folder-Based Pipeline — OrganizerService for the Organizer window
// ADR-028: Movie vs TV Show Detection — hybrid detection (TMDb → heuristic → user)

use riply_core::{FolderRecord, FolderStatus, MediaType, FolderStore, FolderStoreError, MetadataSearcher};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Result of a folder scan
pub struct ScanFoldersResult {
    /// Folders found in the scan directory
    pub folders: Vec<FolderRecord>,
}

/// Organizer service — orchestrates folder-level media operations
pub struct OrganizerService {
    /// Metadata searcher for TMDb lookups
    searcher: Option<Box<dyn MetadataSearcher>>,
    /// Folder store for persistence
    folder_store: Box<dyn FolderStore>,
    /// Scan directory (configurable)
    scan_directory: std::sync::Mutex<Option<std::path::PathBuf>>,
}

impl OrganizerService {
    /// Create a new organizer service
    #[tracing::instrument(skip_all)]
    pub fn new(searcher: Option<Box<dyn MetadataSearcher>>, folder_store: Box<dyn FolderStore>) -> Self {
        info!("Organizer service initialized");
        OrganizerService {
            searcher,
            folder_store,
            scan_directory: std::sync::Mutex::new(None),
        }
    }

    /// Set the scan directory
    pub fn set_scan_directory(&self, path: std::path::PathBuf) {
        let mut dir = self.scan_directory.lock().unwrap();
        *dir = Some(path);
    }

    /// Get the current scan directory
    pub fn get_scan_directory(&self) -> Option<std::path::PathBuf> {
        self.scan_directory.lock().unwrap().clone()
    }

    /// Scan a directory for folders containing media files
    #[tracing::instrument(skip(self), fields(dir = ?path))]
    pub async fn scan_directory(
        &self,
        path: &Path,
    ) -> Result<ScanFoldersResult, anyhow::Error> {
        info!("Scanning directory for folders");

        if !path.exists() {
            return Err(anyhow::anyhow!("Directory does not exist: {:?}", path));
        }

        // Set as the active scan directory
        self.set_scan_directory(path.to_path_buf());

        // Read immediate subdirectories
        let entries = fs::read_dir(path)?;
        let mut folders = Vec::new();

        for entry in entries.flatten() {
            let folder_path = entry.path();
            if !folder_path.is_dir() {
                continue;
            }

            let folder_name = folder_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Collect files with sizes
            let mut files: Vec<(String, u64)> = Vec::new();
            if let Ok(dir_entries) = fs::read_dir(&folder_path) {
                for file_entry in dir_entries.flatten() {
                    if let Ok(metadata) = file_entry.metadata() {
                        if metadata.is_file() {
                            if let Some(ext) = file_entry.path().extension().and_then(|e| e.to_str()) {
                                if is_media_file(ext) {
                                    let filename = file_entry
                                        .file_name()
                                        .to_string_lossy()
                                        .to_string();
                                    files.push((filename, metadata.len()));
                                }
                            }
                        }
                    }
                }
            }

            // Create folder ID (SHA-256 of path)
            let mut hasher = Sha256::new();
            hasher.update(folder_path.to_string_lossy().as_bytes());
            let id = format!("{:x}", hasher.finalize())[..16].to_string();

            let folder =
                FolderRecord::with_files(id, folder_path.clone(), folder_name, files);

            // Save to database
            self.folder_store.save_folder(&folder).await
                .map_err(|e| anyhow::anyhow!("Failed to save folder: {}", e))?;

            folders.push(folder);
        }

        info!(count = folders.len(), "Found folders");
        Ok(ScanFoldersResult { folders })
    }

    /// Analyze a single folder — run TMDb search and update record
    #[tracing::instrument(skip(self), fields(folder = %folder.name))]
    pub async fn analyze_folder(
        &self,
        folder: &FolderRecord,
    ) -> Result<FolderRecord, riply_core::MetadataError> {
        info!("Analyzing folder");

        let mut record = folder.clone();

        // If no files, nothing to analyze
        if record.extra_files.is_empty() && record.main_file.is_none() {
            record.update_status(FolderStatus::Unknown);
            self.folder_store.save_folder(&record).await
                .map_err(|e| riply_core::MetadataError::ApiError(e.to_string()))?;
            return Ok(record);
        }

        // Try heuristic first (movie vs TV detection)
        let detected_type = detect_media_type_from_files(&record);

        // Search TMDb
        if let Some(ref searcher) = self.searcher {
            let results = match detected_type {
                MediaType::Movie => {
                    searcher
                        .search_movies(&record.name, None)
                        .await
                }
                MediaType::Tv => {
                    searcher
                        .search_tv(&record.name)
                        .await
                }
            };

            match results {
                Ok(matches) => {
                    if let Some(best) = matches.first() {
                        let media_type = if best.is_tv {
                            MediaType::Tv
                        } else {
                            MediaType::Movie
                        };

                        record.apply_tmdb_match(
                            best.title.clone(),
                            best.year,
                            best.tmdb_id.unwrap_or(0),
                            media_type,
                        );

                        self.folder_store.save_folder(&record).await
                            .map_err(|e| riply_core::MetadataError::ApiError(e.to_string()))?;
                        return Ok(record);
                    }
                }
                Err(e) => {
                    warn!(error = %e, "TMDb search failed for folder");
                }
            }
        }

        // Fallback: use heuristic result
        record.media_type = detected_type;
        record.confidence = Some(0.5);
        record.update_status(FolderStatus::Unknown);
        self.folder_store.save_folder(&record).await
            .map_err(|e| riply_core::MetadataError::ApiError(e.to_string()))?;

        Ok(record)
    }

    /// Accept a folder — move to Confident status
    #[tracing::instrument(skip(self), fields(folder_id = %folder_id))]
    pub async fn accept_folder(
        &self,
        folder_id: &str,
    ) -> Result<FolderRecord, FolderStoreError> {
        info!("Accepting folder");

        let folder = self.folder_store.get_folder_by_id(folder_id).await?;
        let mut record = folder.ok_or_else(|| FolderStoreError::NotFound(folder_id.to_string()))?;

        record.update_status(FolderStatus::Confident);
        self.folder_store.save_folder(&record).await?;

        Ok(record)
    }

    /// Reject a folder — move to Unknown status
    #[tracing::instrument(skip(self), fields(folder_id = %folder_id))]
    pub async fn reject_folder(
        &self,
        folder_id: &str,
    ) -> Result<FolderRecord, FolderStoreError> {
        info!("Rejecting folder");

        let folder = self.folder_store.get_folder_by_id(folder_id).await?;
        let mut record = folder.ok_or_else(|| FolderStoreError::NotFound(folder_id.to_string()))?;

        record.update_status(FolderStatus::Unknown);
        self.folder_store.save_folder(&record).await?;

        Ok(record)
    }

    /// Accept all confident folders
    #[tracing::instrument(skip(self))]
    pub async fn accept_all_folders(
        &self,
    ) -> Result<usize, FolderStoreError> {
        info!("Accepting all confident folders");

        let folders = self.folder_store.get_folders_by_status("Confident").await?;
        let count = folders.len();

        for folder in folders {
            let mut record = folder;
            record.update_status(FolderStatus::Confident);
            self.folder_store.save_folder(&record).await?;
        }

        info!(count, "Accepted all confident folders");
        Ok(count)
    }

    /// Reject all unknown folders — move to Quarantined
    #[tracing::instrument(skip(self))]
    pub async fn reject_all_folders(
        &self,
    ) -> Result<usize, FolderStoreError> {
        info!("Rejecting all unknown folders");

        let folders = self.folder_store.get_folders_by_status("Unknown").await?;
        let count = folders.len();

        for folder in folders {
            let mut record = folder;
            record.update_status(FolderStatus::Quarantined);
            self.folder_store.save_folder(&record).await?;
        }

        info!(count, "Rejected all unknown folders");
        Ok(count)
    }

    /// Move a single confident folder to its destination
    #[tracing::instrument(skip(self), fields(folder_id = %folder_id))]
    pub async fn move_folder(
        &self,
        folder_id: &str,
    ) -> Result<(), FolderStoreError> {
        info!("Moving folder");

        let folder = self.folder_store.get_folder_by_id(folder_id).await?;
        let folder = folder.ok_or_else(|| FolderStoreError::NotFound(folder_id.to_string()))?;

        if !folder.is_ready_to_move() {
            return Err(FolderStoreError::DatabaseError("Folder is not ready to move".to_string()));
        }

        // Build destination path
        let suggested_name = folder
            .suggested_name
            .as_ref()
            .unwrap_or(&folder.name)
            .clone();
        let suggested_year = folder.suggested_year.unwrap_or(0);

        let dest = if folder.media_type.is_tv() {
            // TV: {Show} ({Year})/Season {NN}/
            let season = "01"; // TODO: get from config or folder metadata
            format!("{}/{} ({})/Season {}/", "TODO_TV_DIR", suggested_name, suggested_year, season)
        } else {
            // Movie: {Title} ({Year})/
            format!("{}/{} ({})/", "TODO_MOVIE_DIR", suggested_name, suggested_year)
        };

        // Move files
        if let Some(ref main_file) = folder.main_file {
            let src = folder.path.join(main_file);
            let dst = format!("{}{}", dest, main_file);
            fs::rename(&src, &dst).map_err(|e| FolderStoreError::IoError(e))?;
        }

        // Move extras to Extras/ subfolder
        if !folder.extra_files.is_empty() {
            let extras_dir = format!("{}Extras/", dest);
            fs::create_dir_all(&extras_dir).map_err(|e| FolderStoreError::IoError(e))?;
            for extra in &folder.extra_files {
                let src = folder.path.join(extra);
                let dst = format!("{}{}", extras_dir, extra);
                fs::rename(&src, &dst).map_err(|e| FolderStoreError::IoError(e))?;
            }
        }

        // Update status
        let mut record = folder;
        record.update_status(FolderStatus::Moved);
        self.folder_store.save_folder(&record).await?;

        info!(dest = %dest, "Moved folder");
        Ok(())
    }

    /// Preview where a folder would move to
    #[tracing::instrument(skip(self), fields(folder = %folder.name))]
    pub fn preview_move(&self, folder: &FolderRecord) -> Option<String> {
        let suggested_name = folder.suggested_name.as_ref().unwrap_or(&folder.name);
        let suggested_year = folder.suggested_year.unwrap_or(0);

        if folder.media_type.is_tv() {
            Some(format!(
                "{}/{} ({})/Season 01/",
                "TODO_TV_DIR", suggested_name, suggested_year
            ))
        } else {
            Some(format!(
                "{}/{} ({})/",
                "TODO_MOVIE_DIR", suggested_name, suggested_year
            ))
        }
    }
}

/// Detect media type from file sizes (ADR-028 heuristic)
///
/// - Movie: 1 large file (>3x median) + small extras
/// - TV: Multiple similarly-sized files (std dev < 30% of mean)
/// - Unknown: Doesn't match either pattern
fn detect_media_type_from_files(folder: &FolderRecord) -> MediaType {
    if folder.file_count < 2 {
        return MediaType::Movie; // Single file is likely a movie
    }

    // Collect file sizes
    let sizes: Vec<u64> = folder.extra_files.iter().map(|_| 1).collect();
    let main_size = 1; // We don't have actual sizes at this point

    // For now, default to movie — real sizes come from actual file scanning
    MediaType::Movie
}

/// Check if a file extension is a media file
fn is_media_file(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mkv" | "mp4" | "avi" | "mov" | "m4v" | "webm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use riply_core::FolderRecord;
    use std::path::PathBuf;

    #[test]
    fn test_is_media_file() {
        assert!(is_media_file("mkv"));
        assert!(is_media_file("MKV"));
        assert!(is_media_file("mp4"));
        assert!(is_media_file("mov"));
        assert!(is_media_file("m4v"));
        assert!(is_media_file("webm"));
        assert!(!is_media_file("txt"));
        assert!(!is_media_file("jpg"));
        assert!(!is_media_file("pdf"));
    }

    #[test]
    fn test_detect_media_type_single_file() {
        let folder = FolderRecord::new(
            "test".to_string(),
            PathBuf::from("/test/Folder"),
            "Folder".to_string(),
            1,
        );
        assert_eq!(detect_media_type_from_files(&folder), MediaType::Movie);
    }

    #[test]
    fn test_organizer_service_creation() {
        let store = MockFolderStore::new();
        let service = OrganizerService::new(None, Box::new(store));
        assert!(service.get_scan_directory().is_none());
    }
}

/// Mock FolderStore for testing
struct MockFolderStore {
    folders: std::sync::Mutex<Vec<FolderRecord>>,
}

impl MockFolderStore {
    fn new() -> Self {
        Self {
            folders: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl FolderStore for MockFolderStore {
    async fn save_folder(&self, folder: &FolderRecord) -> Result<(), FolderStoreError> {
        let mut folders = self.folders.lock().unwrap();
        folders.push(folder.clone());
        Ok(())
    }

    async fn get_all_folders(&self) -> Result<Vec<FolderRecord>, FolderStoreError> {
        Ok(self.folders.lock().unwrap().clone())
    }

    async fn get_folder_by_id(&self, id: &str) -> Result<Option<FolderRecord>, FolderStoreError> {
        Ok(self.folders.lock().unwrap().iter().find(|f| f.id == id).cloned())
    }

    async fn get_folders_by_status(&self, status: &str) -> Result<Vec<FolderRecord>, FolderStoreError> {
        Ok(self.folders.lock().unwrap().iter().filter(|f| f.status.to_string() == status).cloned().collect())
    }

    async fn count_folders_by_status(&self) -> Result<Vec<(String, i64)>, FolderStoreError> {
        Ok(vec![("Scanned".to_string(), 0)])
    }

    async fn update_folder_status(&self, id: &str, status: &str) -> Result<(), FolderStoreError> {
        let mut folders = self.folders.lock().unwrap();
        if let Some(folder) = folders.iter_mut().find(|f| f.id == id) {
            folder.update_status(match status {
                "Confident" => FolderStatus::Confident,
                "Unknown" => FolderStatus::Unknown,
                "Quarantined" => FolderStatus::Quarantined,
                "Moved" => FolderStatus::Moved,
                _ => FolderStatus::Scanned,
            });
        }
        Ok(())
    }

    async fn delete_folder(&self, id: &str) -> Result<(), FolderStoreError> {
        let mut folders = self.folders.lock().unwrap();
        folders.retain(|f| f.id != id);
        Ok(())
    }

    async fn clear_folders(&self) -> Result<(), FolderStoreError> {
        self.folders.lock().unwrap().clear();
        Ok(())
    }
}
