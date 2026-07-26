use app::AnalyzerService;
use infra::{Config, Database, HttpClient};
use riply_core::MetadataSearcher;
use tracing::info;

/// Execute the analyze command
pub async fn analyze_files(db: &Database, config: &Config) -> Result<(), anyhow::Error> {
    info!("Analyzing scanned files");

    // Load scanned files from database
    let records = db.get_files_by_status("Scanned").await?;

    if records.is_empty() {
        println!("No scanned files found. Run 'riply scan' first.");
        return Ok(());
    }

    // Build metadata searcher if API keys are available
    let searcher: Option<Box<dyn MetadataSearcher>> =
        if config.tmdb_api_key.is_some() || config.omdb_api_key.is_some() {
            let client = HttpClient::new(config.tmdb_api_key.clone(), config.omdb_api_key.clone());
            Some(Box::new(client))
        } else {
            println!("No TMDb/OMDb API keys configured — using filename-only analysis.");
            None
        };

    println!("Analyzing {} files...", records.len());
    println!();

    let analyzer = AnalyzerService::new(searcher);
    let mut matched = 0;
    let mut needs_review = 0;
    let mut failed = 0;

    for record in &records {
        let result = analyzer.analyze_file(record).await;

        match result {
            Ok(updated_record) => {
                // Save updated record to database
                db.update_file(&updated_record).await?;

                match updated_record.status {
                    riply_core::PipelineStatus::Matched => {
                        matched += 1;
                        println!(
                            "  {} -> {}",
                            record.filename,
                            updated_record.suggested_title.unwrap_or_default()
                        );
                    }
                    riply_core::PipelineStatus::NeedsReview => {
                        needs_review += 1;
                        println!(
                            "  {} -> {} (needs review)",
                            record.filename,
                            updated_record.suggested_title.unwrap_or_default()
                        );
                    }
                    riply_core::PipelineStatus::Failed => {
                        failed += 1;
                        println!("  {} (failed)", record.filename);
                    }
                    _ => {}
                }
            }
            Err(e) => {
                failed += 1;
                println!("  {} (error: {})", record.filename, e);
            }
        }
    }

    println!();
    println!("Analysis complete:");
    println!("  Matched: {}", matched);
    println!("  Needs review: {}", needs_review);
    println!("  Failed: {}", failed);

    Ok(())
}
