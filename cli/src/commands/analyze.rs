use app::AnalyzerService;
use infra::{Config, Database, HttpClient};
use riply_core::MetadataSearcher;
use serde::Serialize;
use tracing::info;

/// Execute the analyze command
pub async fn analyze_files(
    db: &Database,
    config: &Config,
    json: bool,
) -> Result<(), anyhow::Error> {
    info!("Analyzing scanned files");

    // Load scanned files from database
    let records = db.get_files_by_status("Scanned").await?;

    if records.is_empty() {
        if json {
            let out = AnalyzeOutput {
                files_analyzed: 0,
                matched: 0,
                needs_review: 0,
                failed: 0,
                results: vec![],
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        } else {
            println!("No scanned files found. Run 'riply scan' first.");
        }
        return Ok(());
    }

    // Build metadata searcher if API keys are available
    let searcher: Option<Box<dyn MetadataSearcher>> =
        if config.tmdb_api_key.is_some() || config.omdb_api_key.is_some() {
            let client = HttpClient::new(config.tmdb_api_key.clone(), config.omdb_api_key.clone());
            Some(Box::new(client))
        } else if !json {
            println!("No TMDb/OMDb API keys configured — using filename-only analysis.");
            None
        } else {
            None
        };

    let analyzer = AnalyzerService::new(searcher);
    let mut matched = 0;
    let mut needs_review = 0;
    let mut failed = 0;
    let mut results = vec![];

    for record in &records {
        let result = analyzer.analyze_file(record).await;

        match result {
            Ok(updated_record) => {
                // Save updated record to database
                db.update_file(&updated_record).await?;

                let title = updated_record.suggested_title.clone();
                let year = updated_record.suggested_year;
                let confidence = updated_record.confidence;

                match updated_record.status {
                    riply_core::PipelineStatus::Matched => {
                        matched += 1;
                        if !json {
                            println!(
                                "  {} -> {}",
                                record.filename,
                                title.clone().unwrap_or_default()
                            );
                        }
                        results.push(AnalyzeResult {
                            filename: record.filename.clone(),
                            status: format!("{}", updated_record.status),
                            suggested_title: title,
                            suggested_year: year,
                            confidence,
                        });
                    }
                    riply_core::PipelineStatus::NeedsReview => {
                        needs_review += 1;
                        if !json {
                            println!(
                                "  {} -> {} (needs review)",
                                record.filename,
                                title.clone().unwrap_or_default()
                            );
                        }
                        results.push(AnalyzeResult {
                            filename: record.filename.clone(),
                            status: format!("{}", updated_record.status),
                            suggested_title: title,
                            suggested_year: year,
                            confidence,
                        });
                    }
                    riply_core::PipelineStatus::Failed => {
                        failed += 1;
                        if !json {
                            println!("  {} (failed)", record.filename);
                        }
                        results.push(AnalyzeResult {
                            filename: record.filename.clone(),
                            status: format!("{}", updated_record.status),
                            suggested_title: None,
                            suggested_year: None,
                            confidence: None,
                        });
                    }
                    _ => {}
                }
            }
            Err(e) => {
                failed += 1;
                if !json {
                    println!("  {} (error: {})", record.filename, e);
                }
                results.push(AnalyzeResult {
                    filename: record.filename.clone(),
                    status: format!("Error: {}", e),
                    suggested_title: None,
                    suggested_year: None,
                    confidence: None,
                });
            }
        }
    }

    if json {
        let out = AnalyzeOutput {
            files_analyzed: records.len(),
            matched,
            needs_review,
            failed,
            results,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!();
        println!("Analysis complete:");
        println!("  Matched: {}", matched);
        println!("  Needs review: {}", needs_review);
        println!("  Failed: {}", failed);
    }

    Ok(())
}

/// JSON output for analyze command
#[derive(Serialize)]
struct AnalyzeOutput {
    files_analyzed: usize,
    matched: usize,
    needs_review: usize,
    failed: usize,
    results: Vec<AnalyzeResult>,
}

#[derive(Serialize)]
struct AnalyzeResult {
    filename: String,
    status: String,
    suggested_title: Option<String>,
    suggested_year: Option<u32>,
    confidence: Option<f64>,
}
