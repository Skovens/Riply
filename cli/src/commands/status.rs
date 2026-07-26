use infra::Database;
use tracing::info;

/// Execute the status command
pub async fn show_status(db: &Database) -> Result<(), anyhow::Error> {
    info!("Getting pipeline status");

    let total = db.count_files_by_status("Scanned").await?
        + db.count_files_by_status("Matched").await?
        + db.count_files_by_status("Needs Review").await?
        + db.count_files_by_status("Failed").await?
        + db.count_files_by_status("Confirmed").await?
        + db.count_files_by_status("Moved").await?;

    let scanned = db.count_files_by_status("Scanned").await?;
    let matched = db.count_files_by_status("Matched").await?;
    let needs_review = db.count_files_by_status("Needs Review").await?;
    let failed = db.count_files_by_status("Failed").await?;
    let confirmed = db.count_files_by_status("Confirmed").await?;
    let moved = db.count_files_by_status("Moved").await?;

    println!("Riply Status:");
    println!("  Total files: {}", total);
    println!("  Scanned:     {}", scanned);
    println!("  Matched:     {}", matched);
    println!("  Review:      {}", needs_review);
    println!("  Confirmed:   {}", confirmed);
    println!("  Moved:       {}", moved);
    println!("  Failed:      {}", failed);
    println!();
    println!("  Drives:      (use `riply serve` + web UI for drive info)");
    println!("  Jobs:        (use web UI at http://localhost:3000/#/jobs)");

    Ok(())
}
