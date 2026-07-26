use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use tracing::{debug, info, warn};

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

    /// Scan a directory recursively for media files.
    pub async fn scan_directory(
        &self,
        path: &Path,
    ) -> Result<Vec<riply_core::FileRecord>, anyhow::Error> {
        info!("Scanning directory: {:?}", path);

        if !path.exists() {
            return Err(anyhow::anyhow!("Directory does not exist: {:?}", path));
        }

        let mut records = Vec::new();
        self.walk_directory(path, &mut records)?;

        info!("Found {} media files in {:?}", records.len(), path);
        Ok(records)
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
                warn!("Cannot read directory {:?}: {}", dir, e);
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
                                warn!("Failed to process {:?}: {}", file_path, e);
                            }
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
        record.size = Some(size);

        debug!("Scanned {:?} ({} bytes)", path, size);
        Ok(record)
    }

    /// Hash file contents with SHA-256.
    /// Uses spawn_blocking to avoid blocking the tokio runtime on large files.
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
        let records = scanner.scan_directory(path).await.unwrap();

        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|r| r.filename == "movie.mkv"));
        assert!(records.iter().any(|r| r.filename == "clip.mp4"));
    }

    #[tokio::test]
    async fn test_scan_directory_recursive() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        // Top level
        fs::write(path.join("top.mkv"), "data").unwrap();

        // Subdirectory
        let sub = path.join("Season 01");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("ep01.mkv"), "data").unwrap();
        fs::write(sub.join("ep02.mp4"), "data").unwrap();

        // Nested subdirectory
        let deep = sub.join("extras");
        fs::create_dir(&deep).unwrap();
        fs::write(deep.join("behind_scenes.mkv"), "data").unwrap();

        // Non-media in subdir
        fs::write(sub.join("info.txt"), "data").unwrap();

        let scanner = ScannerService::new();
        let records = scanner.scan_directory(path).await.unwrap();

        assert_eq!(records.len(), 4);
        assert!(records.iter().any(|r| r.filename == "top.mkv"));
        assert!(records.iter().any(|r| r.filename == "ep01.mkv"));
        assert!(records.iter().any(|r| r.filename == "ep02.mp4"));
        assert!(records.iter().any(|r| r.filename == "behind_scenes.mkv"));
    }

    #[tokio::test]
    async fn test_scan_populates_hash_and_size() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        let content = "some media content here";
        fs::write(path.join("test.mkv"), content).unwrap();

        let scanner = ScannerService::new();
        let records = scanner.scan_directory(path).await.unwrap();

        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert!(!r.hash.is_empty());
        assert_eq!(r.hash.len(), 64); // SHA-256 hex = 64 chars
        assert_eq!(r.size, Some(content.len() as i64));
        assert_eq!(r.id.len(), 16); // ID = first 16 hex chars of hash
    }

    #[tokio::test]
    async fn test_scan_id_from_hash_not_filename() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        // Same content, different names -> same hash -> same ID prefix
        fs::write(path.join("copy1.mkv"), "identical").unwrap();
        fs::write(path.join("copy2.mkv"), "identical").unwrap();

        let scanner = ScannerService::new();
        let records = scanner.scan_directory(path).await.unwrap();

        assert_eq!(records.len(), 2);
        // Both have the same hash (and thus same ID prefix)
        assert_eq!(records[0].hash, records[1].hash);
        assert_eq!(records[0].id, records[1].id);
    }

    #[tokio::test]
    async fn test_scan_nonexistent_dir() {
        let scanner = ScannerService::new();
        let result = scanner.scan_directory(Path::new("/nonexistent/path")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_empty_dir() {
        let dir = tempdir().unwrap();
        let scanner = ScannerService::new();
        let records = scanner.scan_directory(dir.path()).await.unwrap();
        assert!(records.is_empty());
    }
}
