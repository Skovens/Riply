use infra::Database;
use riply_core::FileRecord;
use serde::Serialize;
use tracing::info;

/// Execute the duplicate scan command
pub async fn find_duplicates(
    db: &Database,
    _config: &infra::Config,
    json: bool,
) -> Result<(), anyhow::Error> {
    info!("Scanning for duplicate files");

    // Load all files from database
    let all_files = db.get_all_files().await?;

    if all_files.is_empty() {
        if json {
            let out = DuplicateOutput {
                duplicate_groups: 0,
                total_duplicates: 0,
                files_scanned: 0,
                groups: vec![],
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        } else {
            println!("No files found. Run 'riply scan' first.");
        }
        return Ok(());
    }

    // Group files by hash
    let mut hash_groups: std::collections::HashMap<String, Vec<FileRecord>> =
        std::collections::HashMap::new();
    for file in &all_files {
        hash_groups
            .entry(file.hash().to_string())
            .or_default()
            .push(file.clone());
    }

    // Only keep groups with more than one file (actual duplicates)
    let duplicates: Vec<Vec<FileRecord>> = hash_groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();

    let total_duplicates: usize = duplicates.iter().map(|g| g.len() - 1).sum();

    if json {
        let groups: Vec<DuplicateGroupOutput> = duplicates
            .iter()
            .map(|g| DuplicateGroupOutput {
                hash: g[0].hash().to_string(),
                files: g
                    .iter()
                    .map(|f| FileOutput {
                        id: f.id().to_string(),
                        filename: f.filename().to_string(),
                        path: f.path().to_string_lossy().to_string(),
                        size: f.size(),
                    })
                    .collect(),
            })
            .collect();

        let out = DuplicateOutput {
            duplicate_groups: duplicates.len(),
            total_duplicates,
            files_scanned: all_files.len(),
            groups,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        if duplicates.is_empty() {
            println!("No duplicates found.");
        } else {
            println!(
                "Found {} duplicate group(s) with {} total duplicate(s):",
                duplicates.len(),
                total_duplicates
            );
            println!();

            for (i, group) in duplicates.iter().enumerate() {
                let hash_str = group[0].hash();
                println!(
                    "Group {} ({} files, hash: {}...):",
                    i + 1,
                    group.len(),
                    &hash_str[..12.min(hash_str.len())]
                );
                for (j, file) in group.iter().enumerate() {
                    let marker = if j == 0 { "[original]" } else { "[duplicate]" };
                    println!("  {} {} ({:?})", marker, file.filename(), file.path());
                }
                println!();
            }
        }
    }

    Ok(())
}

/// JSON output for duplicate command
#[derive(Serialize)]
struct DuplicateOutput {
    duplicate_groups: usize,
    total_duplicates: usize,
    files_scanned: usize,
    groups: Vec<DuplicateGroupOutput>,
}

#[derive(Serialize)]
struct DuplicateGroupOutput {
    hash: String,
    files: Vec<FileOutput>,
}

#[derive(Serialize)]
struct FileOutput {
    id: String,
    filename: String,
    path: String,
    size: Option<i64>,
}
