use app::ScannerService;
use infra::{Config, Database};
use std::path::Path;
use tracing::info;

/// Execute the scan command
pub async fn scan_directory(directory: &str, config: &Config) -> Result<(), anyhow::Error> {
    info!(dir = %directory, "Scanning directory");

    let path = Path::new(directory);
    if !path.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {}", directory));
    }

    // Initialize database
    let db = Database::new(&config.database_path).await?;
    db.migrate().await?;

    let scanner = ScannerService::new();
    let records = scanner.scan_directory(path).await?;

    if records.is_empty() {
        println!("No media files found in {}", directory);
        return Ok(());
    }

    println!("Found {} media files:", records.len());
    println!();

    // Save each file record to database
    for record in &records {
        db.save_file(record).await?;
        println!("  {} [{}]", record.filename, record.id);
        println!("    Status: {}", record.status);
        println!("    Size: {:?}", record.size);
    }

    println!();
    println!("{} records saved to database", records.len());

    Ok(())
}
