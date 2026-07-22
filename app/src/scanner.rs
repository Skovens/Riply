use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use tracing::info;

/// Scanner service - orchestrates file scanning operations
pub struct ScannerService;

impl Default for ScannerService {
    fn default() -> Self {
        Self::new()
    }
}

impl ScannerService {
    /// Create a new scanner service
    pub fn new() -> Self {
        info!("Scanner service initialized");
        ScannerService
    }

    /// Scan a directory for media files
    pub async fn scan_directory(
        &self,
        path: &Path,
    ) -> Result<Vec<riply_core::FileRecord>, anyhow::Error> {
        info!("Scanning directory: {:?}", path);

        let mut records = Vec::new();

        if !path.exists() {
            return Err(anyhow::anyhow!("Directory does not exist: {:?}", path));
        }

        let entries = fs::read_dir(path)?;
        for entry in entries {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_file() {
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    if is_media_file(ext) {
                        let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
                        let id = compute_id(&filename);
                        // Note: hash field is left empty for now. Actual file hashing
                        // will be implemented in Phase 2 when we integrate ffprobe/sha2.
                        let record = riply_core::FileRecord::new(
                            id,
                            file_path.clone(),
                            filename,
                            String::new(),
                        );
                        records.push(record);
                    }
                }
            }
        }

        info!("Found {} media files in {:?}", records.len(), path);
        Ok(records)
    }

    /// Hash a single file (placeholder for Phase 2)
    pub async fn hash_file(&self, _path: &Path) -> Result<String, anyhow::Error> {
        info!("Hashing file (placeholder)");
        // TODO: Implement actual file hashing using sha2 or similar
        Ok("placeholder_hash".to_string())
    }
}

/// Check if a file extension indicates media content
fn is_media_file(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mkv" | "mp4" | "avi" | "mov" | "m4v" | "webm"
    )
}

/// Compute a deterministic ID from a filename
fn compute_id(filename: &str) -> String {
    let mut hasher = DefaultHasher::new();
    filename.hash(&mut hasher);
    format!("{:x}", hasher.finish())
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
    fn test_compute_id() {
        let id1 = compute_id("movie.mkv");
        let id2 = compute_id("movie.mkv");
        let id3 = compute_id("other.mkv");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[tokio::test]
    async fn test_scan_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        // Create test files
        fs::write(path.join("test.mkv"), "dummy").unwrap();
        fs::write(path.join("test.mp4"), "dummy").unwrap();
        fs::write(path.join("readme.txt"), "dummy").unwrap();

        let scanner = ScannerService::new();
        let records = scanner.scan_directory(path).await.unwrap();

        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|r| r.filename == "test.mkv"));
        assert!(records.iter().any(|r| r.filename == "test.mp4"));
    }
}
