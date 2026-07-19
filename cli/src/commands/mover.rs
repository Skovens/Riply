use app::MoverService;
use infra::Database;
use std::path::PathBuf;
use tracing::info;

/// Execute the move command
pub async fn move_files(db: &Database, config: &infra::Config) -> Result<(), anyhow::Error> {
    info!("Moving confirmed files");
    
    // Load confirmed files from database
    let records = db.get_files_by_status("Confirmed").await?;
    
    if records.is_empty() {
        println!("No confirmed files to move.");
        return Ok(());
    }
    
    let movies_dir = config.movies_directory_buf();
    let tv_dir = config.tv_directory_buf();
    
    // Create directories if they don't exist
    std::fs::create_dir_all(&movies_dir)?;
    std::fs::create_dir_all(&tv_dir)?;
    
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
                // Save updated record to database
                db.update_file(&updated_record).await?;
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