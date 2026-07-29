use std::fs;
use std::path::Path;
use tracing::instrument;
use tracing::{info, warn};

/// Mover service - orchestrates file moving operations
pub struct MoverService;

impl Default for MoverService {
    fn default() -> Self {
        Self::new()
    }
}

impl MoverService {
    /// Create a new mover service
    #[instrument(skip_all)]
    pub fn new() -> Self {
        info!("Mover service initialized");
        MoverService
    }

    /// Move a confirmed file to its organized location
    #[instrument(skip(self))]
    pub async fn move_file(
        &self,
        file: &riply_core::FileRecord,
        movies_dir: &Path,
        tv_dir: &Path,
    ) -> Result<riply_core::FileRecord, anyhow::Error> {
        info!("Moving file: {}", file.filename);

        if !file.is_ready_to_move() {
            return Err(anyhow::anyhow!(
                "File is not ready to move. Status: {:?}",
                file.status
            ));
        }

        // Determine destination path
        let dest_path = self.calculate_destination(file, movies_dir, tv_dir)?;

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Check if destination already exists
        if dest_path.exists() {
            warn!("Destination already exists: {:?}", dest_path);
            return Err(anyhow::anyhow!(
                "Destination already exists: {:?}",
                dest_path
            ));
        }

        // Move the file (with cross-device fallback)
        match fs::rename(&file.path, &dest_path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                info!("Cross-device move detected, falling back to copy+delete");
                fs::copy(&file.path, &dest_path)?;
                let src_size = fs::metadata(&file.path)?.len();
                let dst_size = fs::metadata(&dest_path)?.len();
                if src_size != dst_size {
                    let _ = fs::remove_file(&dest_path);
                    return Err(anyhow::anyhow!(
                        "Copy size mismatch: source {} bytes, destination {} bytes",
                        src_size,
                        dst_size
                    ));
                }
                fs::remove_file(&file.path)?;
            }
            Err(e) => return Err(e.into()),
        }

        // Update the record
        let mut record = file.clone();
        record.update_status(riply_core::PipelineStatus::Moved);

        info!("Successfully moved {} to {:?}", file.filename, dest_path);
        Ok(record)
    }

    /// Calculate the destination path for a file
    fn calculate_destination(
        &self,
        file: &riply_core::FileRecord,
        movies_dir: &Path,
        tv_dir: &Path,
    ) -> Result<std::path::PathBuf, anyhow::Error> {
        let title = file
            .canonical_title
            .clone()
            .or_else(|| file.suggested_title.clone())
            .ok_or_else(|| anyhow::anyhow!("No title available for file: {}", file.filename))?;

        let year = file.canonical_year.or(file.suggested_year);

        let mut dest_path = if file.media_type.is_tv() {
            let series_name = file.series_name.clone().unwrap_or_else(|| title.clone());

            let season = file.season_number.unwrap_or(1);

            // TV Show directory: tv_dir/Series Name/Season XX/
            let season_dir = format!("Season {:02}", season);
            tv_dir.join(&series_name).join(season_dir)
        } else {
            // Movie directory: movies_dir/Title/
            movies_dir.join(&title)
        };

        // Add year to directory name if available
        if let Some(year) = year {
            let parent = dest_path.parent().unwrap_or(&dest_path).to_path_buf();
            let base = dest_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Check if directory already has year in name
            if !base.contains(&year.to_string()) {
                dest_path = parent.join(format!("{} ({})", base, year));
            }
        }

        // Add filename
        dest_path = dest_path.join(&file.filename);

        Ok(dest_path)
    }

    /// Preview where files would be moved without actually moving them
    #[instrument(skip(self))]
    pub async fn preview_move(
        &self,
        file: &riply_core::FileRecord,
        movies_dir: &Path,
        tv_dir: &Path,
    ) -> Result<std::path::PathBuf, anyhow::Error> {
        self.calculate_destination(file, movies_dir, tv_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riply_core::{MediaType, PipelineStatus};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_calculate_destination_movie() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash".to_string(),
        );
        file.canonical_title = Some("Test Movie".to_string());
        file.canonical_year = Some(2023);
        file.status = PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        assert_eq!(dest, movies_dir.join("Test Movie (2023)").join("movie.mkv"));
    }

    #[test]
    fn test_calculate_destination_tv() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/show.s01e01.mkv"),
            "show.s01e01.mkv".to_string(),
            "hash".to_string(),
        );
        file.media_type = MediaType::Tv;
        file.canonical_title = Some("Test Show".to_string());
        file.series_name = Some("Test Show".to_string());
        file.season_number = Some(1);
        file.episode_number = Some(1);
        file.status = PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        assert_eq!(
            dest,
            tv_dir
                .join("Test Show")
                .join("Season 01")
                .join("show.s01e01.mkv")
        );
    }

    #[tokio::test]
    async fn test_move_file() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        // Create source file
        let source_path = dir.path().join("test.mkv");
        fs::write(&source_path, "test content").unwrap();

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            source_path.clone(),
            "test.mkv".to_string(),
            "hash".to_string(),
        );
        file.canonical_title = Some("Test Movie".to_string());
        file.canonical_year = Some(2023);
        file.status = PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let result = mover.move_file(&file, &movies_dir, &tv_dir).await.unwrap();

        // Verify file was moved
        assert!(!source_path.exists());
        assert!(movies_dir
            .join("Test Movie (2023)")
            .join("test.mkv")
            .exists());
        assert_eq!(result.status, PipelineStatus::Moved);
    }
}
