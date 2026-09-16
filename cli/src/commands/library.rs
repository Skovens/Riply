use app::ScannerService;
use infra::{Config, Database};
use riply_core::{MediaType, MetadataSearcher};
use serde::Serialize;
use std::path::Path;
use tracing::info;

/// Execute the library scan command
/// Scans an organized media library (movies/TV shows) and imports files into the database
pub async fn scan_library(
    directory: &str,
    config: &Config,
    media_type: MediaType,
    json: bool,
) -> Result<(), anyhow::Error> {
    info!(dir = %directory, media_type = %media_type, "Scanning library");

    let path = Path::new(directory);
    if !path.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {}", directory));
    }

    let db = Database::new(&config.database_path).await?;

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
    let result = scanner.scan_directory(path, searcher.as_deref()).await?;

    if result.records.is_empty() {
        if json {
            let out = LibraryScanOutput {
                directory: directory.to_string(),
                media_type: media_type.to_string(),
                title_groups: 0,
                files: 0,
                new_files: 0,
                updated_files: 0,
                records: vec![],
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        } else {
            println!("No media files found in {}", directory);
        }
        return Ok(());
    }

    // Save title groups
    let group_count = result.title_groups.len();
    for group in &result.title_groups {
        db.save_title_group(group).await?;
    }

    // Save file records and track new vs updated
    let mut new_files = 0;
    let mut updated_files = 0;

    for record in &result.records {
        // Check if file already exists by hash
        let existing = db.find_file_by_hash(record.hash()).await?;
        if existing.is_some() {
            // Update existing record
            db.update_file(record).await?;
            updated_files += 1;
        } else {
            // Insert new record
            db.save_file(record).await?;
            new_files += 1;
        }
    }

    let total_files = result.records.len();

    if json {
        let records: Vec<serde_json::Value> = result
            .records
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id(),
                    "filename": r.filename(),
                    "path": r.path().to_string_lossy().to_string(),
                    "status": format!("{}", r.status()),
                    "size": r.size(),
                    "title_group_id": r.title_group_id(),
                })
            })
            .collect();
        let out = LibraryScanOutput {
            directory: directory.to_string(),
            media_type: media_type.to_string(),
            title_groups: group_count,
            files: total_files,
            new_files,
            updated_files,
            records,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("Library scan complete for: {}", directory);
        println!("  Media type: {}", media_type);
        println!("  Title groups: {}", group_count);
        println!("  Total files: {}", total_files);
        println!("  New files: {}", new_files);
        println!("  Updated files: {}", updated_files);

        if group_count > 0 {
            println!();
            println!("Title groups:");
            for g in &result.title_groups {
                println!("  [{}] {} ({})", g.id, g.title, g.media_type);
            }
        }
    }

    Ok(())
}

/// JSON output for library scan command
#[derive(Serialize)]
struct LibraryScanOutput {
    directory: String,
    media_type: String,
    title_groups: usize,
    files: usize,
    new_files: usize,
    updated_files: usize,
    records: Vec<serde_json::Value>,
}
