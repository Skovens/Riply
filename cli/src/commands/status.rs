use core::PipelineStatus;
use tracing::info;

/// Execute the status command
pub async fn show_status() -> Result<(), anyhow::Error> {
    info!("Getting pipeline status");
    
    // TODO: Load status from database
    let mut pipeline_status = PipelineStatus::Scanned;
    let mut files_scanned = 0;
    let mut files_matched = 0;
    let mut files_needs_review = 0;
    let mut files_failed = 0;
    let mut files_confirmed = 0;
    let mut files_moved = 0;
    let mut drives = 0;
    let mut active_rips = 0;
    let mut active_moves = 0;
    
    println!("Riply Status:");
    println!("  Pipeline status: {}", pipeline_status);
    println!("  Files scanned: {}", files_scanned);
    println!("  Files matched: {}", files_matched);
    println!("  Files need review: {}", files_needs_review);
    println!("  Files failed: {}", files_failed);
    println!("  Files confirmed: {}", files_confirmed);
    println!("  Files moved: {}", files_moved);
    println!("  Drives: {}", drives);
    println!("  Active rips: {}", active_rips);
    println!("  Active moves: {}", active_moves);
    
    Ok(())
}