use std::str::FromStr;

use sqlx::{sqlite::SqlitePoolOptions, Row};
use tracing::info;

/// Database adapter - handles SQLite connections and queries
pub struct Database {
    pool: sqlx::SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub async fn new(path: &str) -> Result<Self, anyhow::Error> {
        info!(path = %path, "Initializing database");

        let url = format!("sqlite:{}", path);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL").execute(conn).await?;
                    Ok(())
                })
            })
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::from_str(&url)?.create_if_missing(true),
            )
            .await?;

        // Verify connection
        sqlx::query("SELECT 1").execute(&pool).await?;

        Ok(Database { pool })
    }

    /// Apply pending migrations
    pub async fn migrate(&self) -> Result<(), anyhow::Error> {
        info!("Applying database migrations");

        // Read and execute the migration file
        let migration = include_str!("../migrations/001_create_files_table.sql");
        sqlx::query(migration).execute(&self.pool).await?;

        info!("Migrations applied successfully");
        Ok(())
    }

    /// Save a file record.
    /// If a file with the same ID already exists, updates only scan-related
    /// fields (path, filename, size, hash) while preserving pipeline status,
    /// canonical titles, and review decisions.
    pub async fn save_file(&self, file: &riply_core::FileRecord) -> Result<(), anyhow::Error> {
        info!(file = %file.filename, "Saving file record");

        // Check if file already exists
        let existing = sqlx::query_scalar::<_, String>("SELECT id FROM files WHERE id = ?")
            .bind(&file.id)
            .fetch_optional(&self.pool)
            .await?;

        if existing.is_some() {
            // Update only scan-related fields, preserve pipeline state
            sqlx::query(
                r#"
                UPDATE files SET
                    path = ?,
                    filename = ?,
                    size = ?,
                    hash = ?,
                    updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(file.path.to_string_lossy().to_string())
            .bind(&file.filename)
            .bind(file.size)
            .bind(&file.hash)
            .bind(&file.updated_at)
            .bind(&file.id)
            .execute(&self.pool)
            .await?;
        } else {
            // Insert new record
            sqlx::query(
                r#"
                INSERT INTO files (
                    id, path, filename, size, duration, hash, status,
                    suggested_title, suggested_year, confidence,
                    canonical_title, canonical_year, tmdb_id,
                    content_type, media_type, series_name,
                    season_number, episode_number, episode_title,
                    source, subtitle_text, duplicate_path,
                    reviewed, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#,
            )
            .bind(&file.id)
            .bind(file.path.to_string_lossy().to_string())
            .bind(&file.filename)
            .bind(file.size)
            .bind(file.duration)
            .bind(&file.hash)
            .bind(file.status.to_string())
            .bind(&file.suggested_title)
            .bind(file.suggested_year)
            .bind(file.confidence)
            .bind(&file.canonical_title)
            .bind(file.canonical_year)
            .bind(file.tmdb_id)
            .bind(&file.content_type)
            .bind(file.media_type.to_string())
            .bind(&file.series_name)
            .bind(file.season_number)
            .bind(file.episode_number)
            .bind(&file.episode_title)
            .bind(&file.source)
            .bind(&file.subtitle_text)
            .bind(
                file.duplicate_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
            )
            .bind(file.reviewed)
            .bind(&file.created_at)
            .bind(&file.updated_at)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get files by status
    pub async fn get_files_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<riply_core::FileRecord>, anyhow::Error> {
        info!(status = %status, "Getting files by status");

        let rows = sqlx::query(
            r#"
            SELECT
                id, path, filename, size, duration, hash, status,
                suggested_title, suggested_year, confidence,
                canonical_title, canonical_year, tmdb_id,
                content_type, media_type, series_name,
                season_number, episode_number, episode_title,
                source, subtitle_text, duplicate_path,
                reviewed, created_at, updated_at
            FROM files
            WHERE status = ?
            "#,
        )
        .bind(status)
        .fetch_all(&self.pool)
        .await?;

        let mut files = Vec::new();
        for row in rows {
            let file = self.row_to_file_record(&row)?;
            files.push(file);
        }

        Ok(files)
    }

    /// Get all file records
    pub async fn get_all_files(&self) -> Result<Vec<riply_core::FileRecord>, anyhow::Error> {
        info!("Getting all file records");

        let rows = sqlx::query(
            r#"
            SELECT
                id, path, filename, size, duration, hash, status,
                suggested_title, suggested_year, confidence,
                canonical_title, canonical_year, tmdb_id,
                content_type, media_type, series_name,
                season_number, episode_number, episode_title,
                source, subtitle_text, duplicate_path,
                reviewed, created_at, updated_at
            FROM files
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut files = Vec::new();
        for row in rows {
            let file = self.row_to_file_record(&row)?;
            files.push(file);
        }

        Ok(files)
    }

    /// Update a file record
    pub async fn update_file(&self, file: &riply_core::FileRecord) -> Result<(), anyhow::Error> {
        info!(file = %file.filename, "Updating file record");

        sqlx::query(
            r#"
            UPDATE files SET
                path = ?,
                filename = ?,
                size = ?,
                duration = ?,
                hash = ?,
                status = ?,
                suggested_title = ?,
                suggested_year = ?,
                confidence = ?,
                canonical_title = ?,
                canonical_year = ?,
                tmdb_id = ?,
                content_type = ?,
                media_type = ?,
                series_name = ?,
                season_number = ?,
                episode_number = ?,
                episode_title = ?,
                source = ?,
                subtitle_text = ?,
                duplicate_path = ?,
                reviewed = ?,
                created_at = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(file.path.to_string_lossy().to_string())
        .bind(&file.filename)
        .bind(file.size)
        .bind(file.duration)
        .bind(&file.hash)
        .bind(file.status.to_string())
        .bind(&file.suggested_title)
        .bind(file.suggested_year)
        .bind(file.confidence)
        .bind(&file.canonical_title)
        .bind(file.canonical_year)
        .bind(file.tmdb_id)
        .bind(&file.content_type)
        .bind(file.media_type.to_string())
        .bind(&file.series_name)
        .bind(file.season_number)
        .bind(file.episode_number)
        .bind(&file.episode_title)
        .bind(&file.source)
        .bind(&file.subtitle_text)
        .bind(
            file.duplicate_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        )
        .bind(file.reviewed)
        .bind(&file.created_at)
        .bind(&file.updated_at)
        .bind(&file.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Convert a database row to a FileRecord
    fn row_to_file_record(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<riply_core::FileRecord, anyhow::Error> {
        use std::path::PathBuf;

        Ok(riply_core::FileRecord {
            id: row.try_get("id")?,
            path: PathBuf::from(row.try_get::<String, _>("path")?),
            filename: row.try_get("filename")?,
            size: row.try_get("size")?,
            duration: row.try_get("duration")?,
            hash: row.try_get("hash")?,
            status: self.parse_status(row.try_get::<String, _>("status")?),
            suggested_title: row.try_get("suggested_title")?,
            suggested_year: row.try_get("suggested_year")?,
            confidence: row.try_get("confidence")?,
            canonical_title: row.try_get("canonical_title")?,
            canonical_year: row.try_get("canonical_year")?,
            tmdb_id: row.try_get("tmdb_id")?,
            content_type: row.try_get("content_type")?,
            media_type: self.parse_media_type(row.try_get::<String, _>("media_type")?),
            series_name: row.try_get("series_name")?,
            season_number: row.try_get("season_number")?,
            episode_number: row.try_get("episode_number")?,
            episode_title: row.try_get("episode_title")?,
            source: row.try_get("source")?,
            subtitle_text: row.try_get("subtitle_text")?,
            duplicate_path: row
                .try_get("duplicate_path")
                .ok()
                .map(|p: String| PathBuf::from(p)),
            reviewed: row.try_get("reviewed")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    /// Parse pipeline status from string
    fn parse_status(&self, status: String) -> riply_core::PipelineStatus {
        match status.as_str() {
            "Scanned" => riply_core::PipelineStatus::Scanned,
            "Matched" => riply_core::PipelineStatus::Matched,
            "Needs Review" => riply_core::PipelineStatus::NeedsReview,
            "Failed" => riply_core::PipelineStatus::Failed,
            "Confirmed" => riply_core::PipelineStatus::Confirmed,
            "Moved" => riply_core::PipelineStatus::Moved,
            _ => riply_core::PipelineStatus::Scanned,
        }
    }

    /// Parse media type from string
    fn parse_media_type(&self, media_type: String) -> riply_core::MediaType {
        match media_type.as_str() {
            "Movie" => riply_core::MediaType::Movie,
            "Tv" => riply_core::MediaType::Tv,
            _ => riply_core::MediaType::default(),
        }
    }

    /// Get a single file record by ID
    pub async fn get_file_by_id(
        &self,
        id: &str,
    ) -> Result<Option<riply_core::FileRecord>, anyhow::Error> {
        let row = sqlx::query(
            r#"
            SELECT
                id, path, filename, size, duration, hash, status,
                suggested_title, suggested_year, confidence,
                canonical_title, canonical_year, tmdb_id,
                content_type, media_type, series_name,
                season_number, episode_number, episode_title,
                source, subtitle_text, duplicate_path,
                reviewed, created_at, updated_at
            FROM files
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| self.row_to_file_record(&r)).transpose()
    }

    /// Delete a file record by ID
    pub async fn delete_file(&self, id: &str) -> Result<(), anyhow::Error> {
        sqlx::query("DELETE FROM files WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Count files with optional status and search filters
    pub async fn count_files(
        &self,
        status: Option<&str>,
        search: Option<&str>,
    ) -> Result<i64, anyhow::Error> {
        let mut sql = "SELECT COUNT(*) as count FROM files".to_string();
        let has_filters = status.is_some() || search.is_some();

        if has_filters {
            sql.push_str(" WHERE 1=1");
            if status.is_some() {
                sql.push_str(" AND status = ?");
            }
            if search.is_some() {
                sql.push_str(" AND (filename LIKE ? OR canonical_title LIKE ?)");
            }
        }

        if let Some(s) = status {
            if let Some(search_term) = search {
                let pattern = format!("%{}%", search_term);
                let row = sqlx::query(&sql)
                    .bind(s)
                    .bind(&pattern)
                    .bind(&pattern)
                    .fetch_one(&self.pool)
                    .await?;
                return Ok(row.try_get("count")?);
            }
            let row = sqlx::query(&sql).bind(s).fetch_one(&self.pool).await?;
            Ok(row.try_get("count")?)
        } else if let Some(search_term) = search {
            let pattern = format!("%{}%", search_term);
            let row = sqlx::query(&sql)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_one(&self.pool)
                .await?;
            Ok(row.try_get("count")?)
        } else {
            let row = sqlx::query(&sql).fetch_one(&self.pool).await?;
            Ok(row.try_get("count")?)
        }
    }

    /// Get paginated files with optional status and search filters
    pub async fn get_files_paginated(
        &self,
        status: Option<&str>,
        search: Option<&str>,
        page: u32,
        per_page: u32,
    ) -> Result<(Vec<riply_core::FileRecord>, i64), anyhow::Error> {
        let offset = (page - 1) * per_page;

        let count = self.count_files(status, search).await?;

        let base_sql = r#"
            SELECT
                id, path, filename, size, duration, hash, status,
                suggested_title, suggested_year, confidence,
                canonical_title, canonical_year, tmdb_id,
                content_type, media_type, series_name,
                season_number, episode_number, episode_title,
                source, subtitle_text, duplicate_path,
                reviewed, created_at, updated_at
            FROM files
        "#;

        let order_sql = " ORDER BY updated_at DESC LIMIT ? OFFSET ?";

        if let Some(s) = status {
            if let Some(search_term) = search {
                let pattern = format!("%{}%", search_term);
                let sql = format!(
                    "{} WHERE 1=1 AND status = ? AND (filename LIKE ? OR canonical_title LIKE ?){}",
                    base_sql, order_sql
                );
                let rows = sqlx::query(&sql)
                    .bind(s)
                    .bind(&pattern)
                    .bind(&pattern)
                    .bind(per_page as i64)
                    .bind(offset as i64)
                    .fetch_all(&self.pool)
                    .await?;
                let files: Vec<_> = rows
                    .iter()
                    .map(|r| self.row_to_file_record(r))
                    .collect::<Result<_, _>>()?;
                return Ok((files, count));
            }
            let sql = format!("{} WHERE 1=1 AND status = ?{}", base_sql, order_sql);
            let rows = sqlx::query(&sql)
                .bind(s)
                .bind(per_page as i64)
                .bind(offset as i64)
                .fetch_all(&self.pool)
                .await?;
            let files: Vec<_> = rows
                .iter()
                .map(|r| self.row_to_file_record(r))
                .collect::<Result<_, _>>()?;
            return Ok((files, count));
        }

        if let Some(search_term) = search {
            let pattern = format!("%{}%", search_term);
            let sql = format!(
                "{} WHERE 1=1 AND (filename LIKE ? OR canonical_title LIKE ?){}",
                base_sql, order_sql
            );
            let rows = sqlx::query(&sql)
                .bind(&pattern)
                .bind(&pattern)
                .bind(per_page as i64)
                .bind(offset as i64)
                .fetch_all(&self.pool)
                .await?;
            let files: Vec<_> = rows
                .iter()
                .map(|r| self.row_to_file_record(r))
                .collect::<Result<_, _>>()?;
            return Ok((files, count));
        }

        let sql = format!("{}{}", base_sql, order_sql);
        let rows = sqlx::query(&sql)
            .bind(per_page as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?;
        let files: Vec<_> = rows
            .iter()
            .map(|r| self.row_to_file_record(r))
            .collect::<Result<_, _>>()?;
        Ok((files, count))
    }

    /// Count files by status
    pub async fn count_files_by_status(&self, status: &str) -> Result<i64, anyhow::Error> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM files WHERE status = ?")
            .bind(status)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("count")?)
    }

    /// Count files grouped by all statuses in a single query.
    /// Returns a map of status name -> count.
    pub async fn count_files_by_status_all(
        &self,
    ) -> Result<std::collections::HashMap<String, i64>, anyhow::Error> {
        let rows = sqlx::query("SELECT status, COUNT(*) as count FROM files GROUP BY status")
            .fetch_all(&self.pool)
            .await?;

        let mut counts = std::collections::HashMap::new();
        for row in rows {
            let status: String = row.try_get("status")?;
            let count: i64 = row.try_get("count")?;
            counts.insert(status, count);
        }
        Ok(counts)
    }

    /// Find a file record by its hash
    pub async fn find_file_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<riply_core::FileRecord>, anyhow::Error> {
        let row = sqlx::query(
            r#"
            SELECT
                id, path, filename, size, duration, hash, status,
                suggested_title, suggested_year, confidence,
                canonical_title, canonical_year, tmdb_id,
                content_type, media_type, series_name,
                season_number, episode_number, episode_title,
                source, subtitle_text, duplicate_path,
                reviewed, created_at, updated_at
            FROM files
            WHERE hash = ?
            "#,
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| self.row_to_file_record(&r)).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_connection() {
        let db = Database::new(":memory:").await.unwrap();
        assert!(db.pool.size() > 0);
    }

    #[tokio::test]
    async fn test_save_and_get_file() {
        let db = Database::new(":memory:").await.unwrap();
        db.migrate().await.unwrap();

        // Create a test file record
        let file = riply_core::FileRecord::new(
            "test123".to_string(),
            std::path::PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        // Save the file
        db.save_file(&file).await.unwrap();

        // Get files by status
        let files = db.get_files_by_status("Scanned").await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].filename, "movie.mkv");
    }
}
