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
    #[instrument(skip(self), fields(file = %file.filename()))]
    pub async fn move_file(
        &self,
        file: &riply_core::FileRecord,
        movies_dir: &Path,
        tv_dir: &Path,
    ) -> Result<riply_core::FileRecord, anyhow::Error> {
        info!("Moving file");

        if !file.is_ready_to_move() {
            return Err(anyhow::anyhow!(
                "File is not ready to move. Status: {:?}",
                file.status()
            ));
        }

        // Determine destination path
        let dest_path = self.calculate_destination(file, movies_dir, tv_dir)?;

        // Check if destination already exists — apply Copy-N suffix
        let dest_path = {
            let folder = dest_path.parent().unwrap_or(Path::new("."));
            let filename = dest_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());

            // Check if the folder already existed (before we create it)
            let folder_existed = folder.exists() && folder.is_dir();

            if folder_existed {
                // Folder already exists — rename folder with Copy-N
                let folder_name = folder
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "folder".to_string());
                let folder_parent = folder.parent().unwrap_or(Path::new("."));

                let mut copy_n = 1;
                loop {
                    let new_folder_name = format!("{} (Copy {})", folder_name, copy_n);
                    let new_folder = folder_parent.join(&new_folder_name);
                    if !new_folder.exists() {
                        // Create the new folder and place file inside
                        break new_folder.join(&filename);
                    }
                    copy_n += 1;
                }
            } else {
                dest_path
            }
        };

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Move the file (with cross-device fallback)
        match fs::rename(file.path(), &dest_path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                info!("Cross-device move detected, falling back to copy+delete");
                fs::copy(file.path(), &dest_path)?;
                let src_size = fs::metadata(file.path())?.len();
                let dst_size = fs::metadata(&dest_path)?.len();
                if src_size != dst_size {
                    let _ = fs::remove_file(&dest_path);
                    return Err(anyhow::anyhow!(
                        "Copy size mismatch: source {} bytes, destination {} bytes",
                        src_size,
                        dst_size
                    ));
                }
                fs::remove_file(file.path())?;
            }
            Err(e) => return Err(e.into()),
        }

        // Update the record
        let mut record = file.clone();
        record.update_status(riply_core::PipelineStatus::Moved);

        info!(dest = ?dest_path, "Successfully moved file");
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
            .canonical_title()
            .clone()
            .or_else(|| file.suggested_title().clone())
            .ok_or_else(|| anyhow::anyhow!("No title available for file: {}", file.filename()))?;

        let year = file.canonical_year().as_ref().copied().or(file.suggested_year().as_ref().copied());

        // Build base path with year if available
        let base_path = if file.media_type().is_tv() {
            let series_name = file.series_name().clone().unwrap_or_else(|| title.clone());
            let season = *file.season_number().as_ref().unwrap_or(&1);
            let season_dir = format!("Season {:02}", season);
            let mut base = tv_dir.join(&series_name).join(season_dir);

            // Add year to series name if available
            if let Some(year) = year {
                let parent = base.parent().unwrap_or(&base).to_path_buf();
                let base_name = base
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if !base_name.contains(&year.to_string()) {
                    base = parent.join(format!("{} ({})", base_name, year));
                }
            }

            // Extras go into an Extras/ subdirectory within the season.
            if file.is_extra() {
                base.join("Extras")
            } else {
                base
            }
        } else {
            let mut base = movies_dir.join(&title);

            // Add year to title directory if available.
            if let Some(year) = year {
                let parent = base.parent().unwrap_or(&base).to_path_buf();
                let base_name = base
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if !base_name.contains(&year.to_string()) {
                    base = parent.join(format!("{} ({})", base_name, year));
                }
            }

            // Movie extras go into an Extras/ subdirectory.
            if file.is_extra() {
                base.join("Extras")
            } else {
                base
            }
        };

        // Determine the output filename: extras keep original name, main titles
        // are renamed per Jellyfin convention.
        let ext = file
            .filename()
            .rsplit_once('.')
            .map(|(_, ext)| format!(".{}", ext))
            .unwrap_or_default();
        let output_filename: String = if file.is_extra() {
            file.filename().to_string()
        } else if file.media_type().is_tv() {
            let season = *file.season_number().as_ref().unwrap_or(&1);
            let episode = *file.episode_number().as_ref().unwrap_or(&1);
            format!("{} - S{:02}E{:02}{}", title, season, episode, ext)
        } else {
            format!("{}{}", title, ext)
        };

        let dest_path = base_path.join(&output_filename);

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
            std::path::PathBuf::from("/test/inception_2010.1080p.bluray.mkv"),
            "inception_2010.1080p.bluray.mkv".to_string(),
            "hash".to_string(),
        );
        *file.canonical_title_mut() =Some("Inception".to_string());
        *file.canonical_year_mut() =Some(2010);
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        // Jellyfin: main movie renamed to {Title}.{ext}
        assert_eq!(
            dest,
            movies_dir.join("Inception (2010)").join("Inception.mkv")
        );
    }

    #[test]
    fn test_calculate_destination_tv() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/breaking.bad.s01e01.720p.hdtv.mkv"),
            "breaking.bad.s01e01.720p.hdtv.mkv".to_string(),
            "hash".to_string(),
        );
        *file.media_type_mut() =MediaType::Tv;
        *file.canonical_title_mut() =Some("Breaking Bad".to_string());
        *file.series_name_mut() =Some("Breaking Bad".to_string());
        *file.season_number_mut() =Some(1);
        *file.episode_number_mut() =Some(1);
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        // Jellyfin: TV episode renamed to {Title} - S{NN}E{NN}.{ext}
        assert_eq!(
            dest,
            tv_dir
                .join("Breaking Bad")
                .join("Season 01")
                .join("Breaking Bad - S01E01.mkv")
        );
    }

    #[tokio::test]
    async fn test_move_file() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        // Create source file
        let source_path = dir.path().join("matrix.1999.1080p.bluray.mkv");
        fs::write(&source_path, "test content").unwrap();

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            source_path.clone(),
            "matrix.1999.1080p.bluray.mkv".to_string(),
            "hash".to_string(),
        );
        *file.canonical_title_mut() =Some("The Matrix".to_string());
        *file.canonical_year_mut() =Some(1999);
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let result = mover.move_file(&file, &movies_dir, &tv_dir).await.unwrap();

        // Verify file was moved and renamed to Jellyfin format
        assert!(!source_path.exists());
        assert!(movies_dir
            .join("The Matrix (1999)")
            .join("The Matrix.mkv")
            .exists());
        assert_eq!(*result.status(), PipelineStatus::Moved);
    }

    #[test]
    fn test_calculate_destination_movie_extra() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/behind_scenes.mkv"),
            "behind_scenes.mkv".to_string(),
            "hash".to_string(),
        );
        *file.canonical_title_mut() =Some("Test Movie".to_string());
        *file.canonical_year_mut() =Some(2023);
        *file.content_type_mut() =riply_core::ContentType::Extra;
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        // Extras keep original filename, go into capitalized Extras/ (Jellyfin convention)
        assert_eq!(
            dest,
            movies_dir
                .join("Test Movie (2023)")
                .join("Extras")
                .join("behind_scenes.mkv")
        );
    }

    #[test]
    fn test_calculate_destination_tv_extra() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/commentary.mkv"),
            "commentary.mkv".to_string(),
            "hash".to_string(),
        );
        *file.media_type_mut() =MediaType::Tv;
        *file.canonical_title_mut() =Some("Test Show".to_string());
        *file.series_name_mut() =Some("Test Show".to_string());
        *file.season_number_mut() =Some(1);
        *file.content_type_mut() =riply_core::ContentType::Extra;
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        // Extras keep original filename, go into capitalized Extras/ (Jellyfin convention)
        assert_eq!(
            dest,
            tv_dir
                .join("Test Show")
                .join("Season 01")
                .join("Extras")
                .join("commentary.mkv")
        );
    }

    #[test]
    fn test_calculate_destination_tv_episode_not_extra() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/show.s01e01.mkv"),
            "show.s01e01.mkv".to_string(),
            "hash".to_string(),
        );
        *file.media_type_mut() =MediaType::Tv;
        *file.canonical_title_mut() =Some("Test Show".to_string());
        *file.series_name_mut() =Some("Test Show".to_string());
        *file.season_number_mut() =Some(1);
        *file.episode_number_mut() =Some(1);
        *file.content_type_mut() =riply_core::ContentType::Main;
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        // Main TV episode renamed per Jellyfin convention
        assert_eq!(
            dest,
            tv_dir
                .join("Test Show")
                .join("Season 01")
                .join("Test Show - S01E01.mkv")
        );
    }

    #[test]
    fn test_calculate_destination_no_extension() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/inception"),
            "inception".to_string(),
            "hash".to_string(),
        );
        *file.canonical_title_mut() =Some("Inception".to_string());
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let dest = mover
            .calculate_destination(&file, &movies_dir, &tv_dir)
            .unwrap();

        // File with no extension gets title-only name (no trailing dot)
        assert_eq!(
            dest,
            movies_dir.join("Inception").join("Inception")
        );
    }

    #[tokio::test]
    async fn test_move_file_copy_n_suffix() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        // Create destination directory that already exists
        let existing = movies_dir.join("Speed (1994)");
        fs::create_dir_all(&existing).unwrap();

        // Create source file
        let source_path = dir.path().join("speed_1994.mkv");
        fs::write(&source_path, "test content").unwrap();

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            source_path.clone(),
            "speed_1994.mkv".to_string(),
            "hash".to_string(),
        );
        *file.canonical_title_mut() =Some("Speed".to_string());
        *file.canonical_year_mut() =Some(1994);
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let result = mover.move_file(&file, &movies_dir, &tv_dir).await.unwrap();

        // Should use Copy-N suffix since Speed (1994) already exists
        let expected = movies_dir.join("Speed (1994) (Copy 1)").join("Speed.mkv");
        assert!(expected.exists());
        assert!(!source_path.exists());
        assert_eq!(*result.status(), PipelineStatus::Moved);
    }

    #[tokio::test]
    async fn test_move_file_copy_n_suffix_chain() {
        let dir = tempdir().unwrap();
        let movies_dir = dir.path().join("movies");
        let tv_dir = dir.path().join("tv");

        // Create existing directories with Copy-N chain
        fs::create_dir_all(movies_dir.join("Speed (1994)")).unwrap();
        fs::create_dir_all(movies_dir.join("Speed (1994) (Copy 1)")).unwrap();
        fs::create_dir_all(movies_dir.join("Speed (1994) (Copy 2)")).unwrap();

        // Create source file
        let source_path = dir.path().join("speed_2.mkv");
        fs::write(&source_path, "test content").unwrap();

        let mut file = riply_core::FileRecord::new(
            "test".to_string(),
            source_path.clone(),
            "speed_2.mkv".to_string(),
            "hash".to_string(),
        );
        *file.canonical_title_mut() =Some("Speed".to_string());
        *file.canonical_year_mut() =Some(1994);
        *file.status_mut() =PipelineStatus::Confirmed;

        let mover = MoverService::new();
        let result = mover.move_file(&file, &movies_dir, &tv_dir).await.unwrap();

        // Should skip to Copy 3
        let expected = movies_dir.join("Speed (1994) (Copy 3)").join("Speed.mkv");
        assert!(expected.exists());
        assert_eq!(*result.status(), PipelineStatus::Moved);
    }
}
