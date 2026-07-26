use infra::Database;
use tracing::info;

/// Execute the status command
pub async fn show_status(db: &Database) -> Result<(), anyhow::Error> {
    info!("Getting pipeline status");

    let counts = db.count_files_by_status_all().await?;

    let total: i64 = counts.values().sum();

    println!("Riply Status:");
    println!("  Total files: {}", total);
    println!(
        "  Scanned:     {}",
        counts.get("Scanned").copied().unwrap_or(0)
    );
    println!(
        "  Matched:     {}",
        counts.get("Matched").copied().unwrap_or(0)
    );
    println!(
        "  Review:      {}",
        counts.get("Needs Review").copied().unwrap_or(0)
    );
    println!(
        "  Confirmed:   {}",
        counts.get("Confirmed").copied().unwrap_or(0)
    );
    println!(
        "  Moved:       {}",
        counts.get("Moved").copied().unwrap_or(0)
    );
    println!(
        "  Failed:      {}",
        counts.get("Failed").copied().unwrap_or(0)
    );
    println!();
    println!("  Drives:      (use `riply serve` + web UI for drive info)");
    println!("  Jobs:        (use web UI at http://localhost:3000/#/jobs)");

    Ok(())
}
