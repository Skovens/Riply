use infra::Database;
use tracing::info;

/// Execute the status command
pub async fn show_status(db: &Database) -> Result<(), anyhow::Error> {
    info!("Getting pipeline status");

    // Get file counts by status
    let all_files = db.get_all_files().await?;

    let mut files_scanned = 0;
    let mut files_matched = 0;
    let mut files_needs_review = 0;
    let mut files_failed = 0;
    let mut files_confirmed = 0;
    let mut files_moved = 0;

    for file in &all_files {
        match file.status {
            riply_core::PipelineStatus::Scanned => files_scanned += 1,
            riply_core::PipelineStatus::Matched => files_matched += 1,
            riply_core::PipelineStatus::NeedsReview => files_needs_review += 1,
            riply_core::PipelineStatus::Failed => files_failed += 1,
            riply_core::PipelineStatus::Confirmed => files_confirmed += 1,
            riply_core::PipelineStatus::Moved => files_moved += 1,
        }
    }

    println!("Riply Status:");
    println!("  Total files: {}", all_files.len());
    println!("  Files scanned: {}", files_scanned);
    println!("  Files matched: {}", files_matched);
    println!("  Files need review: {}", files_needs_review);
    println!("  Files failed: {}", files_failed);
    println!("  Files confirmed: {}", files_confirmed);
    println!("  Files moved: {}", files_moved);
    println!("  Drives: 0");
    println!("  Active rips: 0");
    println!("  Active moves: 0");

    Ok(())
}
