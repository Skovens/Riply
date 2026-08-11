use app::MoverService;
use infra::Database;
use serde::Serialize;
use tracing::info;

/// Execute the move command
pub async fn move_files(
    db: &Database,
    config: &infra::Config,
    json: bool,
) -> Result<(), anyhow::Error> {
    info!("Moving confirmed files");

    // Load confirmed files from database
    let records = db.get_files_by_status("Confirmed").await?;

    if records.is_empty() {
        if json {
            let out = MoveOutput {
                movies_dir: config.movies_directory_buf().to_string_lossy().to_string(),
                tv_dir: config.tv_directory_buf().to_string_lossy().to_string(),
                files_processed: 0,
                moved: 0,
                skipped: 0,
                errors: 0,
                results: vec![],
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        } else {
            println!("No confirmed files to move.");
        }
        return Ok(());
    }

    let movies_dir = config.movies_directory_buf();
    let tv_dir = config.tv_directory_buf();

    // Create directories if they don't exist
    std::fs::create_dir_all(&movies_dir)?;
    std::fs::create_dir_all(&tv_dir)?;

    let mover = MoverService::new();
    let mut moved = 0;
    let mut skipped = 0;
    let mut errors = 0;
    let mut results = vec![];

    if !json {
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(records.len() as u64);
        pb.set_message("Moving files...");
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("#>-"),
        );

        for record in &records {
            if !record.is_ready_to_move() {
                skipped += 1;
                pb.inc(1);
                continue;
            }

            match mover.move_file(record, &movies_dir, &tv_dir).await {
                Ok(updated_record) => {
                    moved += 1;
                    db.update_file(&updated_record).await?;
                    println!("  ✓ {} -> {:?}", record.filename, updated_record.path);
                    results.push(MoveResult {
                        filename: record.filename.clone(),
                        status: "moved".to_string(),
                        path: Some(updated_record.path.to_string_lossy().to_string()),
                    });
                }
                Err(e) => {
                    errors += 1;
                    println!("  ✗ {} (error: {})", record.filename, e);
                    results.push(MoveResult {
                        filename: record.filename.clone(),
                        status: format!("error: {}", e),
                        path: None,
                    });
                }
            }
            pb.inc(1);
        }
        pb.finish_with_message("Move complete");

        println!();
        println!("Move complete:");
        println!("  Moved: {}", moved);
        println!("  Skipped: {}", skipped);
        println!("  Errors: {}", errors);
    } else {
        for record in &records {
            if !record.is_ready_to_move() {
                skipped += 1;
                continue;
            }

            match mover.move_file(record, &movies_dir, &tv_dir).await {
                Ok(updated_record) => {
                    moved += 1;
                    db.update_file(&updated_record).await?;
                    results.push(MoveResult {
                        filename: record.filename.clone(),
                        status: "moved".to_string(),
                        path: Some(updated_record.path.to_string_lossy().to_string()),
                    });
                }
                Err(e) => {
                    errors += 1;
                    results.push(MoveResult {
                        filename: record.filename.clone(),
                        status: format!("error: {}", e),
                        path: None,
                    });
                }
            }
        }

        let out = MoveOutput {
            movies_dir: movies_dir.to_string_lossy().to_string(),
            tv_dir: tv_dir.to_string_lossy().to_string(),
            files_processed: records.len(),
            moved,
            skipped,
            errors,
            results,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    }

    Ok(())
}

/// JSON output for move command
#[derive(Serialize)]
struct MoveOutput {
    movies_dir: String,
    tv_dir: String,
    files_processed: usize,
    moved: usize,
    skipped: usize,
    errors: usize,
    results: Vec<MoveResult>,
}

#[derive(Serialize)]
struct MoveResult {
    filename: String,
    status: String,
    path: Option<String>,
}
