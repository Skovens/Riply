use app::ScannerService;
use infra::{Config, Database};
use riply_core::{MediaType, MetadataSearcher};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tracing::{error, info, warn};

/// Watch a directory for new files and automatically scan them
pub async fn watch_directory(
    directory: &str,
    config: &Config,
    media_type: MediaType,
) -> Result<(), anyhow::Error> {
    info!(dir = %directory, media_type = %media_type, "Starting directory watch");

    let path = Path::new(directory);
    if !path.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {}", directory));
    }

    let db = Database::new(&config.database_path).await?;
    db.migrate().await?;

    // Initialize metadata searcher if API keys are configured
    let searcher: Option<Box<dyn MetadataSearcher>> =
        if config.tmdb_api_key.is_some() || config.omdb_api_key.is_some() {
            let client =
                infra::HttpClient::new(config.tmdb_api_key.clone(), config.omdb_api_key.clone());
            Some(Box::new(client))
        } else {
            None
        };

    let scanner = ScannerService::new();

    // Track seen files to detect new ones
    let mut seen_files: HashSet<String> = HashSet::new();

    // Populate with existing files
    if let Ok(entries) = path.read_dir() {
        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.is_file() {
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    if is_media_file(ext) {
                        seen_files.insert(file_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    info!("Watching directory: {}", directory);
    info!("Current files: {}", seen_files.len());
    info!("Press Ctrl+C to stop");

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Scan directory for new files
        match path.read_dir() {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let file_path = entry.path();
                    if file_path.is_file() {
                        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                            if !is_media_file(ext) {
                                continue;
                            }
                        } else {
                            continue;
                        }

                        let file_key = file_path.to_string_lossy().to_string();
                        if !seen_files.contains(&file_key) {
                            seen_files.insert(file_key.clone());
                            let filename = file_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();

                            info!(file = %filename, "Detected new file");

                            // Small delay to ensure file is fully written
                            tokio::time::sleep(Duration::from_secs(2)).await;

                            if !file_path.exists() {
                                warn!(file = %filename, "File disappeared before processing");
                                seen_files.remove(&file_key);
                                continue;
                            }

                            // Scan the directory (it will pick up the new file)
                            match scanner.scan_directory(path, searcher.as_deref()).await {
                                Ok(result) => {
                                    // Save title groups
                                    for group in &result.title_groups {
                                        if let Err(e) = db.save_title_group(group).await {
                                            error!(group = %group.title, error = %e, "Failed to save title group");
                                        }
                                    }

                                    // Save file records
                                    let mut new_count = 0;
                                    for record in &result.records {
                                        // Check if file already exists
                                        let existing = db.find_file_by_hash(&record.hash).await;
                                        match existing {
                                            Ok(Some(_)) => {
                                                // Update existing
                                                if let Err(e) = db.update_file(record).await {
                                                    error!(file = %record.filename, error = %e, "Failed to update file");
                                                }
                                            }
                                            Ok(None) => {
                                                // Insert new
                                                if let Err(e) = db.save_file(record).await {
                                                    error!(file = %record.filename, error = %e, "Failed to save file");
                                                } else {
                                                    new_count += 1;
                                                }
                                            }
                                            Err(e) => {
                                                error!(file = %record.filename, error = %e, "Failed to check file existence");
                                            }
                                        }
                                    }

                                    if new_count > 0 {
                                        info!(file = %filename, new_files = new_count, "Successfully processed new file");
                                    }
                                }
                                Err(e) => {
                                    error!(file = %filename, error = %e, "Failed to scan directory");
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to read directory");
            }
        }
    }
}

/// Check if a file extension indicates media content.
fn is_media_file(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mkv" | "mp4" | "avi" | "mov" | "m4v" | "webm"
    )
}
