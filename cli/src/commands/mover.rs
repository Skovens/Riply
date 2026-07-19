use app::MoverService;
use core::FileRecord;
use std::path::PathBuf;
use tracing::info;

/// Execute the move command
pub async fn move_files(
    movies_dir: Option<String>,
    tv_dir: Option<String>,
) -> Result<(), anyhow::Error> {
    info!("Moving confirmed files");
    
    let movies_dir = PathBuf::from(movies_dir.unwrap_or_else(|| "movies".to_string()));
    let tv_dir = PathBuf::from(tv_dir.unwrap_or_else(|| "tv".to_string()));
    
    // Create directories if they don't exist
    std::fs::create_dir_all(&movies_dir)?;
    std::fs::create_dir_all(&tv_dir)?;
    
    // TODO: Load confirmed files from database
    let records: Vec<FileRecord> = Vec::new();
    
    if records.is_empty() {
        println!("No confirmed files to move.");
        return Ok(());
    }
    
    println!("Moving {} files to organized library:", records.len());
    println!("  Movies: {}", movies_dir.display());
    println!("  TV: {}", tv_dir.display());
    println!();
    
    let mover = MoverService::new();
    let mut moved = 0;
    let mut skipped = 0;
    let mut errors = 0;
    
    for record in &records {
        if !record.is_ready_to_move() {
            skipped += 1;
            continue;
        }
        
        match mover.move_file(record, &movies_dir, &tv_dir).await {
            Ok(updated_record) => {
                moved += 1;
                println!("  ✓ {} -> {:?}", record.filename, updated_record.path);
            }
            Err(e) => {
                errors += 1;
                println!("  ✗ {} (error: {})", record.filename, e);
            }
        }
    }
    
    println!();
    println!("Move complete:");
    println!("  Moved: {}", moved);
    println!("  Skipped: {}", skipped);
    println!("  Errors: {}", errors);
    
    Ok(())
}