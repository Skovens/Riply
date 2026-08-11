use app::ScannerService;
use infra::{Config, Database};
use riply_core::MetadataSearcher;
use serde::Serialize;
use std::path::Path;
use tracing::info;

/// Execute the scan command
pub async fn scan_directory(
    directory: &str,
    config: &Config,
    searcher: Option<&dyn MetadataSearcher>,
    json: bool,
) -> Result<(), anyhow::Error> {
    info!(dir = %directory, "Scanning directory");

    let path = Path::new(directory);
    if !path.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {}", directory));
    }

    // Initialize database
    let db = Database::new(&config.database_path).await?;
    db.migrate().await?;

    let scanner = ScannerService::new();

    if !json {
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new_spinner();
        pb.set_message("Scanning directory...");
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let result = scanner.scan_directory(path, searcher).await?;

        pb.finish_with_message("Scan complete");

        // Save title groups with progress
        if !result.title_groups.is_empty() {
            let group_pb = ProgressBar::new(result.title_groups.len() as u64);
            group_pb.set_message("Saving title groups...");
            group_pb.set_style(ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("#>-"));
            for group in &result.title_groups {
                db.save_title_group(group).await?;
                group_pb.inc(1);
            }
            group_pb.finish_with_message("Title groups saved");
        }

        if result.records.is_empty() {
            println!("No media files found in {}", directory);
            return Ok(());
        }

        let group_count = result.title_groups.len();
        if group_count > 0 {
            println!("Found {} title group(s):", group_count);
            for g in &result.title_groups {
                println!("  [{}] {}", g.id, g.title);
            }
            println!();
        }

        println!("Found {} media files:", result.records.len());
        println!();

        // Save each file record to database
        let file_pb = ProgressBar::new(result.records.len() as u64);
        file_pb.set_message("Saving files...");
        file_pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("#>-"),
        );

        for record in &result.records {
            db.save_file(record).await?;
            file_pb.inc(1);
        }
        file_pb.finish_with_message("Files saved");

        println!();
        println!("{} records saved to database", result.records.len());
    } else {
        let result = scanner.scan_directory(path, searcher).await?;

        // Save title groups
        for group in &result.title_groups {
            db.save_title_group(group).await?;
        }

        if result.records.is_empty() {
            let out = ScanOutput {
                directory: directory.to_string(),
                title_groups: 0,
                files: 0,
                records: vec![],
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
            return Ok(());
        }

        let group_count = result.title_groups.len();
        let records: Vec<serde_json::Value> = result
            .records
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "filename": r.filename,
                    "status": format!("{}", r.status),
                    "size": r.size,
                })
            })
            .collect();
        let out = ScanOutput {
            directory: directory.to_string(),
            title_groups: group_count,
            files: result.records.len(),
            records,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    }

    Ok(())
}

/// JSON output for scan command
#[derive(Serialize)]
struct ScanOutput {
    directory: String,
    title_groups: usize,
    files: usize,
    records: Vec<serde_json::Value>,
}
