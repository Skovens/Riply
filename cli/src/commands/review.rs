use std::io::{self, Write};

use infra::Database;
use riply_core::PipelineStatus;
use tracing::info;

/// Execute the review command
pub async fn review_files(yolo: bool, db: &Database) -> Result<(), anyhow::Error> {
    info!(yolo, "Reviewing files");

    // Load files that need review from database
    let records = db.get_files_by_status("Needs Review").await?;

    if records.is_empty() {
        println!("No files need review.");
        return Ok(());
    }

    if yolo {
        println!("YOLO mode enabled - confirming all files without review");
        confirm_all(&records, db).await?;
        return Ok(());
    }

    // Track which files have been confirmed (by original index)
    let mut confirmed = vec![false; records.len()];

    loop {
        let remaining: Vec<(usize, &riply_core::FileRecord)> = records
            .iter()
            .enumerate()
            .filter(|(i, _)| !confirmed[*i])
            .collect();

        if remaining.is_empty() {
            println!("\nAll files reviewed.");
            break;
        }

        println!("\n{} file(s) remaining:", remaining.len());
        for (original_idx, record) in &remaining {
            let confidence = record.confidence().as_ref().copied().unwrap_or(0.0);
            println!("  [{}] {}", original_idx + 1, record.filename());
            println!(
                "      Suggested: {} ({})",
                record.suggested_title().as_deref().unwrap_or("Unknown"),
                record.suggested_year().as_ref().copied().unwrap_or(0)
            );
            println!("      Confidence: {:.1}%", confidence * 100.0);
        }

        println!();
        println!("  Enter numbers to confirm (e.g. 1,3,5)");
        println!("  'all' to confirm remaining  |  'skip' to quit  |  'q' to quit");
        print!("> ");
        io::stdout().flush()?;

        let stdin = io::stdin();
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let input = line.trim().to_lowercase();

        match input.as_str() {
            "skip" | "q" | "quit" | "" => {
                let confirmed_count = confirmed.iter().filter(|&&c| c).count();
                println!(
                    "Done. {}/{} files confirmed.",
                    confirmed_count,
                    records.len()
                );
                break;
            }
            "all" => {
                confirm_remaining(&remaining, db).await?;
                for (i, _) in &remaining {
                    confirmed[*i] = true;
                }
                println!("All remaining files confirmed.");
                let confirmed_count = confirmed.iter().filter(|&&c| c).count();
                if confirmed_count == records.len() {
                    println!("\nAll files reviewed.");
                }
                break;
            }
            _ => {
                let indices = parse_indices(&input, records.len());
                if indices.is_empty() {
                    println!("No valid numbers. Try again.");
                    continue;
                }

                let to_confirm: Vec<(usize, &riply_core::FileRecord)> = indices
                    .iter()
                    .filter(|&&i| !confirmed[i - 1])
                    .map(|&i| (i - 1, &records[i - 1]))
                    .collect();

                if to_confirm.is_empty() {
                    println!("All selected files already confirmed.");
                    continue;
                }

                for &(idx, record) in &to_confirm {
                    let mut updated = record.clone();
                    updated.update_status(PipelineStatus::Confirmed);
                    db.update_file(&updated).await?;
                    confirmed[idx] = true;
                    info!(file = %record.filename(), "Confirmed");
                }

                println!("{} file(s) confirmed.", to_confirm.len());
            }
        }
    }

    Ok(())
}

/// Confirm all files without individual review
async fn confirm_all(
    records: &[riply_core::FileRecord],
    db: &Database,
) -> Result<(), anyhow::Error> {
    info!(count = records.len(), "Confirming all files");

    for record in records {
        let mut updated = record.clone();
        updated.update_status(PipelineStatus::Confirmed);
        db.update_file(&updated).await?;
        info!(file = %record.filename(), "Confirmed");
    }

    println!("All {} files confirmed", records.len());
    Ok(())
}

/// Confirm remaining files in a batch
async fn confirm_remaining(
    remaining: &[(usize, &riply_core::FileRecord)],
    db: &Database,
) -> Result<(), anyhow::Error> {
    for &(_, record) in remaining {
        let mut updated = record.clone();
        updated.update_status(PipelineStatus::Confirmed);
        db.update_file(&updated).await?;
        info!(file = %record.filename(), "Confirmed");
    }
    Ok(())
}

/// Parse comma-separated 1-based indices from user input
fn parse_indices(input: &str, max: usize) -> Vec<usize> {
    input
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .filter(|&n| n >= 1 && n <= max)
        .collect()
}
