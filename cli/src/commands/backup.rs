use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use infra::Database;
use tracing::info;

/// Execute the backup command: create a timestamped snapshot of the database.
pub async fn backup_database(db: &Database, output: Option<PathBuf>) -> Result<(), anyhow::Error> {
    info!("Backing up database");

    let destination = match output {
        Some(path) => path,
        None => {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            PathBuf::from(format!("riply_backup_{}.db", timestamp))
        }
    };

    db.backup(&destination).await?;
    println!("Database backed up to {}", destination.display());
    Ok(())
}

/// Execute the restore command: replace the current database from a backup file.
pub async fn restore_database(db: &Database, source: PathBuf) -> Result<(), anyhow::Error> {
    info!(source = %source.display(), "Restoring database");

    if !source.exists() {
        anyhow::bail!("Backup file not found: {}", source.display());
    }

    db.restore(&source).await?;
    println!("Database restored from {}", source.display());
    Ok(())
}
