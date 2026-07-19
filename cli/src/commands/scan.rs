use crate::core;
use app::ScannerService;
use std::path::Path;
use tracing::info;

/// Execute the scan command
pub async fn scan_directory(directory: &str) -> Result<(), anyhow::Error> {
    info!("Scanning directory: {}", directory);
    
    let path = Path::new(directory);
    if !path.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {}", directory));
    }
    
    let scanner = ScannerService::new();
    let records = scanner.scan_directory(path).await?;
    
    if records.is_empty() {
        println!("No media files found in {}", directory);
        return Ok(());
    }
    
    println!("Found {} media files:", records.len());
    println!();
    
    for record in &records {
        println!("  {} [{}]", record.filename, record.id);
        println!("    Status: {}", record.status);
        println!("    Size: {:?}", record.size);
    }
    
    // TODO: Save records to database for later processing
    println!();
    println!("{} records saved to pipeline", records.len());
    
    Ok(())
}