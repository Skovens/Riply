use app::AnalyzerService;
use infra::Database;
use tracing::info;

/// Execute the review command
pub async fn review_files(yolo: bool, db: &Database) -> Result<(), anyhow::Error> {
    info!("Reviewing files (yolo={})", yolo);
    
    // Load files that need review from database
    let records = db.get_files_by_status("Needs Review").await?;
    
    if records.is_empty() {
        println!("No files need review.");
        return Ok(());
    }
    
    println!("Files needing review:");
    println!();
    
    for (i, record) in records.iter().enumerate() {
        let confidence = record.confidence.unwrap_or(0.0);
        println!("  [{}] {}", i + 1, record.filename);
        println!("      Suggested: {} ({})", 
            record.suggested_title.as_deref().unwrap_or("Unknown"),
            record.suggested_year.unwrap_or(0)
        );
        println!("      Confidence: {:.1}%", confidence * 100.0);
        println!();
    }
    
    if yolo {
        println!("YOLO mode enabled - confirming all files without review");
        confirm_all(&records, db).await?;
    } else {
        println!("Manual review mode:");
        println!("  Enter file numbers to confirm (comma-separated)");
        println!("  Enter 'all' to confirm all");
        println!("  Enter 'skip' to skip all");
        println!();
        // TODO: Read user input and confirm/skip files
        println!("Review complete (manual mode not yet implemented)");
    }
    
    Ok(())
}

/// Confirm all files without individual review
async fn confirm_all(records: &[core::FileRecord], db: &Database) -> Result<(), anyhow::Error> {
    info!("Confirming all {} files", records.len());
    
    for record in records {
        let mut updated = record.clone();
        updated.update_status(core::PipelineStatus::Confirmed);
        db.update_file(&updated).await?;
        info!("Confirmed: {}", record.filename);
    }
    
    println!("All {} files confirmed", records.len());
    Ok(())
}