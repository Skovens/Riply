use app::AnalyzerService;
use core::FileRecord;
use tracing::info;

/// Execute the analyze command
pub async fn analyze_files() -> Result<(), anyhow::Error> {
    info!("Analyzing scanned files");
    
    // TODO: Load scanned records from database
    let records: Vec<FileRecord> = Vec::new();
    
    if records.is_empty() {
        println!("No scanned files found. Run 'riply scan' first.");
        return Ok(());
    }
    
    println!("Analyzing {} files...", records.len());
    println!();
    
    let analyzer = AnalyzerService::new();
    let mut matched = 0;
    let mut needs_review = 0;
    let mut failed = 0;
    
    for record in &records {
        let result = analyzer.analyze_file(record).await;
        
        match result {
            Ok(updated_record) => {
                match updated_record.status {
                    core::PipelineStatus::Matched => {
                        matched += 1;
                        println!("  ✓ {} -> {}", record.filename, updated_record.suggested_title.unwrap_or_default());
                    }
                    core::PipelineStatus::NeedsReview => {
                        needs_review += 1;
                        println!("  ? {} -> {} (needs review)", record.filename, updated_record.suggested_title.unwrap_or_default());
                    }
                    core::PipelineStatus::Failed => {
                        failed += 1;
                        println!("  ✗ {} (failed)", record.filename);
                    }
                    _ => {}
                }
            }
            Err(e) => {
                failed += 1;
                println!("  ✗ {} (error: {})", record.filename, e);
            }
        }
    }
    
    println!();
    println!("Analysis complete:");
    println!("  Matched: {}", matched);
    println!("  Needs review: {}", needs_review);
    println!("  Failed: {}", failed);
    
    // TODO: Save updated records to database
    Ok(())
}