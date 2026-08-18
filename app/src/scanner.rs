use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

use riply_core::{MetadataSearcher, NameCleaner, TitleGroup};
use tracing::instrument;
use tracing::{debug, info, warn};

/// Scanner service - orchestrates file scanning operations
pub struct ScannerService;

/// Result of a folder-first scan, containing both the title groups and
/// the individual file records grouped under them.
pub struct ScanResult {
    pub title_groups: Vec<TitleGroup>,
    pub records: Vec<riply_core::FileRecord>,
}

impl Default for ScannerService {
    fn default() -> Self {
        Self::new()
    }
}

impl ScannerService {
    /// Create a new scanner service
    #[instrument(skip_all)]
    pub fn new() -> Self {
        info!("Scanner service initialized");
        ScannerService
    }

    /// Scan a directory for media files.
    ///
    /// If the directory has immediate subdirectories, each subdirectory is
    /// treated as a `TitleGroup` and scanned individually (folder-first mode).
    /// Otherwise, falls back to flat recursive scanning.
    #[instrument(skip(self, searcher), fields(dir = ?path))]
    pub async fn scan_directory(
        &self,
        path: &Path,
        searcher: Option<&dyn MetadataSearcher>,
    ) -> Result<ScanResult, anyhow::Error> {
        info!("Scanning directory");

        if !path.exists() {
            return Err(anyhow::anyhow!("Directory does not exist: {:?}", path));
        }

        // Check for immediate subdirectories → folder-first mode
        let entries = fs::read_dir(path)?;
        let subdirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect();

        if subdirs.is_empty() {
            // Fall back to flat scanning
            let mut records = Vec::new();
            self.walk_directory(path, &mut records)?;
            info!(count = records.len(), "Found media files (flat mode)");
            return Ok(ScanResult {
                title_groups: vec![],
                records,
            });
        }

        // Folder-first: scan each subdirectory as a TitleGroup
        // Also collect root-level files (without a group)
        let mut all_records = Vec::new();
        let mut all_groups = Vec::new();

        // Scan root-level files
        if let Ok(dir_entries) = fs::read_dir(path) {
            for file_entry in dir_entries.flatten() {
                let file_path = file_entry.path();
                if file_path.is_file() {
                    if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                        if is_media_file(ext) {
                            if let Ok(record) = self.process_file(&file_path) {
                                all_records.push(record);
                            }
                        }
                    }
                }
            }
        }

        for entry in &subdirs {
            let dir_path = entry.path();
            let dir_name = dir_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Use NameCleaner on directory name
            let cleaned = NameCleaner::clean_name(&dir_name);
            let search_query = NameCleaner::clean_name_for_search(&dir_name);

            // Optionally resolve with metadata searcher
            let (title, year, tmdb_id) = if let Some(searcher) = searcher {
                match searcher
                    .search(&search_query, None, &riply_core::MediaType::Movie)
                    .await
                {
                    Ok(results) => {
                        if let Some(m) = results.first() {
                            (m.title.clone(), m.year, m.tmdb_id)
                        } else {
                            (cleaned, None, None)
                        }
                    }
                    Err(_) => (cleaned, None, None),
                }
            } else {
                (cleaned, None, None)
            };

            let dir_str = dir_path.to_string_lossy();
            let group_id = {
                let mut hasher = Sha256::new();
                hasher.update(dir_str.as_bytes());
                let hash = format!("{:x}", hasher.finalize());
                hash[..16].to_string()
            };

            let mut group = TitleGroup::new(group_id, dir_path.clone(), title);
            group.year = year.map(|y| y as u16);
            group.tmdb_id = tmdb_id;

            // Walk all files within the group's directory (including nested subdirs)
            let mut group_records = Vec::new();
            self.walk_group_directory(&dir_path, &group.id, &mut group_records)?;

            all_groups.push(group);
            all_records.extend(group_records);
        }

        info!(
            groups = all_groups.len(),
            files = all_records.len(),
            "Found media files (folder-first mode)"
        );
        Ok(ScanResult {
            title_groups: all_groups,
            records: all_records,
        })
    }

    /// Recursively walk a directory and collect media files.
    fn walk_directory(
        &self,
        dir: &Path,
        records: &mut Vec<riply_core::FileRecord>,
    ) -> Result<(), anyhow::Error> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(dir = ?dir, error = %e, "Cannot read directory");
                return Ok(());
            }
        };

        for entry in entries {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_dir() {
                self.walk_directory(&file_path, records)?;
            } else if file_path.is_file() {
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    if is_media_file(ext) {
                        match self.process_file(&file_path) {
                            Ok(record) => records.push(record),
                            Err(e) => {
                                warn!(file = ?file_path, error = %e, "Failed to process file");
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Walk a group's directory recursively and collect media files with the given title_group_id.
    fn walk_group_directory(
        &self,
        dir: &Path,
        group_id: &str,
        records: &mut Vec<riply_core::FileRecord>,
    ) -> Result<(), anyhow::Error> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(dir = ?dir, error = %e, "Cannot read group directory");
                return Ok(());
            }
        };

        for entry in entries {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_dir() {
                self.walk_group_directory(&file_path, group_id, records)?;
            } else if file_path.is_file() {
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    if is_media_file(ext) {
                        if let Ok(mut record) = self.process_file(&file_path) {
                            *record.title_group_id_mut() =Some(group_id.to_string());
                            records.push(record);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Process a single file: hash it, read metadata, build a FileRecord.
    fn process_file(&self, path: &Path) -> Result<riply_core::FileRecord, anyhow::Error> {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("No filename for {:?}", path))?
            .to_string_lossy()
            .to_string();

        let metadata = fs::metadata(path)?;
        let size = metadata.len() as i64;

        let hash = hash_file(path)?;
        let id = hash[..16].to_string();

        let record = riply_core::FileRecord::new(id, path.to_path_buf(), filename, hash);
        // FileRecord::new sets size=None, so patch it in.
        let mut record = record;
        *record.size_mut() =Some(size);

        debug!(file = ?path, size, "Scanned file");
        Ok(record)
    }

    /// Hash file contents with SHA-256.
    /// Uses spawn_blocking to avoid blocking the tokio runtime on large files.
    #[instrument(fields(path = ?path))]
    pub async fn hash_file_async(path: &Path) -> Result<String, anyhow::Error> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || hash_file(&path))
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }
}

/// Compute SHA-256 hash of a file, returning the hex string.
fn hash_file(path: &Path) -> Result<String, anyhow::Error> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Check if a file extension indicates media content.
fn is_media_file(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mkv" | "mp4" | "avi" | "mov" | "m4v" | "webm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_is_media_file() {
        assert!(is_media_file("mkv"));
        assert!(is_media_file("MKV"));
        assert!(is_media_file("mp4"));
        assert!(!is_media_file("txt"));
        assert!(!is_media_file("jpg"));
    }

    #[test]
    fn test_hash_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let hash = hash_file(&path).unwrap();
        // SHA-256 of "hello world" is well-known
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_hash_deterministic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("file.mkv");
        fs::write(&path, "content").unwrap();

        let h1 = hash_file(&path).unwrap();
        let h2 = hash_file(&path).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_differs_by_content() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.mkv");
        let b = dir.path().join("b.mkv");
        fs::write(&a, "alpha").unwrap();
        fs::write(&b, "beta").unwrap();

        assert_ne!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }

    #[tokio::test]
    async fn test_scan_directory_flat() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        fs::write(path.join("movie.mkv"), "data").unwrap();
        fs::write(path.join("clip.mp4"), "data").unwrap();
        fs::write(path.join("readme.txt"), "data").unwrap();

        let scanner = ScannerService::new();
        let result = scanner.scan_directory(path, None).await.unwrap();

        assert!(result.title_groups.is_empty());
        assert_eq!(result.records.len(), 2);
        assert!(result.records.iter().any(|r| r.filename() == "movie.mkv"));
        assert!(result.records.iter().any(|r| r.filename() == "clip.mp4"));
    }

    #[tokio::test]
    async fn test_scan_directory_recursive() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        // Top level
        fs::write(path.join("top.mkv"), "data").unwrap();

        // Subdirectory (this triggers folder-first mode, so it will be a group)
        let sub = path.join("Season 01");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("ep01.mkv"), "data").unwrap();
        fs::write(sub.join("ep02.mp4"), "data").unwrap();

        // Nested within subdir
        let deep = sub.join("extras");
        fs::create_dir(&deep).unwrap();
        fs::write(deep.join("behind_scenes.mkv"), "data").unwrap();

        // Non-media in subdir
        fs::write(sub.join("info.txt"), "data").unwrap();

        let scanner = ScannerService::new();
        let result = scanner.scan_directory(path, None).await.unwrap();

        // One group for "Season 01" subdirectory
        assert_eq!(result.title_groups.len(), 1);
        assert_eq!(result.title_groups[0].title, "Season 01");

        // Files at root: top.mkv; in the group: ep01.mkv, ep02.mp4, behind_scenes.mkv
        assert_eq!(result.records.len(), 4);
        assert!(result.records.iter().any(|r| r.filename() == "ep01.mkv"));
        assert!(result.records.iter().any(|r| r.filename() == "ep02.mp4"));
        assert!(result
            .records
            .iter()
            .any(|r| r.filename() == "behind_scenes.mkv"));

        // Root-level file has no group; files in subdirectory have group set
        assert!(result
            .records
            .iter()
            .any(|rec| rec.filename() == "top.mkv" && rec.title_group_id().is_none()));
        for rec in result
            .records
            .iter()
            .filter(|rec| rec.filename() != "top.mkv")
        {
            assert_eq!(
                rec.title_group_id().as_deref(),
                Some(result.title_groups[0].id.as_str())
            );
        }
    }

    #[tokio::test]
    async fn test_scan_populates_hash_and_size() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        let content = "some media content here";
        fs::write(path.join("test.mkv"), content).unwrap();

        let scanner = ScannerService::new();
        let result = scanner.scan_directory(path, None).await.unwrap();

        assert_eq!(result.records.len(), 1);
        let r = &result.records[0];
        assert!(!r.hash().is_empty());
        assert_eq!(r.hash().len(), 64); // SHA-256 hex = 64 chars
        assert_eq!(r.size(), Some(content.len() as i64));
        assert_eq!(r.id().len(), 16); // ID = first 16 hex chars of hash
    }

    #[tokio::test]
    async fn test_scan_id_from_hash_not_filename() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        // Same content, different names -> same hash -> same ID prefix
        fs::write(path.join("copy1.mkv"), "identical").unwrap();
        fs::write(path.join("copy2.mkv"), "identical").unwrap();

        let scanner = ScannerService::new();
        let result = scanner.scan_directory(path, None).await.unwrap();

        assert_eq!(result.records.len(), 2);
        // Both have the same hash (and thus same ID prefix)
        assert_eq!(result.records[0].hash(), result.records[1].hash());
        assert_eq!(result.records[0].id(), result.records[1].id());
    }

    #[tokio::test]
    async fn test_scan_nonexistent_dir() {
        let scanner = ScannerService::new();
        let result = scanner
            .scan_directory(Path::new("/nonexistent/path"), None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_empty_dir() {
        let dir = tempdir().unwrap();
        let scanner = ScannerService::new();
        let result = scanner.scan_directory(dir.path(), None).await.unwrap();
        assert!(result.records.is_empty());
    }

    #[tokio::test]
    async fn test_scan_folder_first_no_searcher() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        let movie_dir = path.join("Inception 2010");
        fs::create_dir(&movie_dir).unwrap();
        fs::write(movie_dir.join("title_t00.mkv"), "data").unwrap();
        fs::write(movie_dir.join("title_t01.mkv"), "data").unwrap();

        let scanner = ScannerService::new();
        let result = scanner.scan_directory(path, None).await.unwrap();

        assert_eq!(result.title_groups.len(), 1);
        assert_eq!(result.title_groups[0].title, "Inception 2010");
        assert_eq!(result.records.len(), 2);
        for r in &result.records {
            assert_eq!(
                r.title_group_id().as_deref(),
                Some(result.title_groups[0].id.as_str())
            );
        }
    }
}
