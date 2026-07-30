use app::ScannerService;
use infra::{Config, Database};
use riply_core::MetadataSearcher;
use std::path::Path;
use tracing::info;

/// Execute the scan command
pub async fn scan_directory(
    directory: &str,
    config: &Config,
    searcher: Option<&dyn MetadataSearcher>,
) -> Result<(), anyhow::Error> {
    info!(dir = %directory, "Scanning directory");

    let path = Path::new(directory);
    if !path.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {}", directory));
    }

    // Initialize database
    let db = Database::new(&config.database_path).await?;
    db.migrate().await?;

    let scanner = ScannerService::new();
    let result = scanner.scan_directory(path, searcher).await?;

    // Save title groups
    for group in &result.title_groups {
        db.save_title_group(group).await?;
    }

    if result.records.is_empty() {
        println!("No media files found in {}", directory);
        return Ok(());
    }

    let group_count = result.title_groups.len();
    if group_count > 0 {
        println!("Found {} title group(s):", group_count);
        for g in &result.title_groups {
            println!("  [{}] {}", g.id, g.title);
        }
        println!();
    }

    println!("Found {} media files:", result.records.len());
    println!();

    // Save each file record to database
    for record in &result.records {
        db.save_file(record).await?;
        println!("  {} [{}]", record.filename, record.id);
        println!("    Status: {}", record.status);
        println!("    Size: {:?}", record.size);
    }

    println!();
    println!("{} records saved to database", result.records.len());

    Ok(())
}
