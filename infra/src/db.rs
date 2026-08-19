use std::path::Path;
use std::str::FromStr;

use sqlx::{sqlite::SqlitePoolOptions, Acquire, Row};
use tracing::info;

/// Encode a string list (genres/cast) as a JSON TEXT column value.
fn json_encode_list(value: &Option<Vec<String>>) -> Option<String> {
    value
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
}

/// Decode a JSON TEXT column value back into a string list.
fn json_decode_list(value: Option<String>) -> Option<Vec<String>> {
    value.and_then(|s| serde_json::from_str(&s).ok())
}

/// Database adapter - handles SQLite connections and queries
pub struct Database {
    pool: sqlx::SqlitePool,
    db_path: String,
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

        Ok(Database {
            pool,
            db_path: path.to_string(),
        })
    }

    /// Apply pending migrations
    pub async fn migrate(&self) -> Result<(), anyhow::Error> {
        info!("Applying database migrations");

        sqlx::migrate!("src/migrations").run(&self.pool).await?;

        info!("Migrations applied successfully");
        Ok(())
    }

    /// Check if the database is reachable
    pub async fn ping(&self) -> Result<(), anyhow::Error> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    /// Save a file record.
    /// If a file with the same ID already exists, updates only scan-related
    /// fields (path, filename, size, hash) while preserving pipeline status,
    /// canonical titles, and review decisions.
    pub async fn save_file(&self, file: &riply_core::FileRecord) -> Result<(), anyhow::Error> {
        info!(file = %file.filename(), "Saving file record");

        // Check if file already exists
        let existing = sqlx::query_scalar::<_, String>("SELECT id FROM files WHERE id = ?")
            .bind(&file.id())
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
            .bind(file.path().to_string_lossy().to_string())
            .bind(&file.filename())
            .bind(file.size())
            .bind(&file.hash())
            .bind(&file.updated_at())
            .bind(&file.id())
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
                    title_group_id, genres, runtime, content_rating,
                    overview, cast_list,
                    reviewed, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#,
            )
            .bind(&file.id())
            .bind(file.path().to_string_lossy().to_string())
            .bind(&file.filename())
            .bind(file.size())
            .bind(file.duration())
            .bind(&file.hash())
            .bind(file.status().to_string())
            .bind(&file.suggested_title())
            .bind(file.suggested_year())
            .bind(file.confidence())
            .bind(&file.canonical_title())
            .bind(file.canonical_year())
            .bind(file.tmdb_id())
            .bind(riply_core::content_type_to_str(&file.content_type()))
            .bind(file.media_type().to_string())
            .bind(&file.series_name())
            .bind(file.season_number())
            .bind(file.episode_number())
            .bind(&file.episode_title())
            .bind(&file.source())
            .bind(&file.subtitle_text())
            .bind(
                file.duplicate_path()
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
            )
            .bind(&file.title_group_id())
            .bind(json_encode_list(&file.genres()))
            .bind(file.runtime())
            .bind(&file.content_rating())
            .bind(&file.overview())
            .bind(json_encode_list(&file.cast_list()))
            .bind(file.reviewed())
            .bind(&file.created_at())
            .bind(&file.updated_at())
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
                title_group_id, genres, runtime, content_rating,
                overview, cast_list,
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
                title_group_id, genres, runtime, content_rating,
                overview, cast_list,
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
        info!(file = %file.filename(), "Updating file record");

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
                title_group_id = ?,
                genres = ?,
                runtime = ?,
                content_rating = ?,
                overview = ?,
                cast_list = ?,
                reviewed = ?,
                created_at = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(file.path().to_string_lossy().to_string())
        .bind(&file.filename())
        .bind(file.size())
        .bind(file.duration())
        .bind(&file.hash())
        .bind(file.status().to_string())
        .bind(&file.suggested_title())
        .bind(file.suggested_year())
        .bind(file.confidence())
        .bind(&file.canonical_title())
        .bind(file.canonical_year())
        .bind(file.tmdb_id())
        .bind(riply_core::content_type_to_str(&file.content_type()))
        .bind(file.media_type().to_string())
        .bind(&file.series_name())
        .bind(file.season_number())
        .bind(file.episode_number())
        .bind(&file.episode_title())
        .bind(&file.source())
        .bind(&file.subtitle_text())
        .bind(
            file.duplicate_path()
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        )
        .bind(&file.title_group_id())
        .bind(json_encode_list(&file.genres()))
        .bind(file.runtime())
        .bind(&file.content_rating())
        .bind(&file.overview())
        .bind(json_encode_list(&file.cast_list()))
        .bind(file.reviewed())
        .bind(&file.created_at())
        .bind(&file.updated_at())
        .bind(&file.id())
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
            scan: riply_core::ScanRecord {
                id: row.try_get("id")?,
                path: PathBuf::from(row.try_get::<String, _>("path")?),
                filename: row.try_get("filename")?,
                size: row.try_get("size")?,
                duration: row.try_get("duration")?,
                hash: row.try_get("hash")?,
                status: self.parse_status(row.try_get::<String, _>("status")?),
                content_type: riply_core::ContentType::from_str(&row.try_get::<String, _>("content_type")?),
                media_type: self.parse_media_type(row.try_get::<String, _>("media_type")?),
                title_group_id: row.try_get::<Option<String>, _>("title_group_id")?,
                duplicate_path: row
                    .try_get::<Option<String>, _>("duplicate_path")?
                    .map(PathBuf::from),
                reviewed: row.try_get("reviewed")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            },
            metadata: riply_core::MetadataRecord {
                suggested_title: row.try_get("suggested_title")?,
                suggested_year: row.try_get("suggested_year")?,
                confidence: row.try_get("confidence")?,
                canonical_title: row.try_get("canonical_title")?,
                canonical_year: row.try_get("canonical_year")?,
                tmdb_id: row.try_get("tmdb_id")?,
                series_name: row.try_get("series_name")?,
                season_number: row.try_get("season_number")?,
                episode_number: row.try_get("episode_number")?,
                episode_title: row.try_get("episode_title")?,
                source: row.try_get("source")?,
            },
            enrichment: riply_core::EnrichmentRecord {
                subtitle_text: row.try_get("subtitle_text")?,
                genres: json_decode_list(row.try_get("genres")?),
                runtime: row.try_get("runtime")?,
                content_rating: row.try_get("content_rating")?,
                overview: row.try_get("overview")?,
                cast_list: json_decode_list(row.try_get("cast_list")?),
            },
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
                title_group_id, genres, runtime, content_rating,
                overview, cast_list,
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

    /// Delete all file records and title groups (DB reset)
    pub async fn clear_all(&self) -> Result<usize, anyhow::Error> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query("DELETE FROM files").execute(&mut *tx).await?;
        let files_deleted = result.rows_affected() as usize;

        sqlx::query("DELETE FROM title_groups")
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(files_deleted)
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
                title_group_id, genres, runtime, content_rating,
                overview, cast_list,
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

    /// Save a title group
    pub async fn save_title_group(
        &self,
        group: &riply_core::TitleGroup,
    ) -> Result<(), anyhow::Error> {
        sqlx::query(
            r#"
            INSERT INTO title_groups (id, source_directory, title, year, tmdb_id, media_type, series_name, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                source_directory = excluded.source_directory,
                title = excluded.title,
                year = excluded.year,
                tmdb_id = excluded.tmdb_id,
                media_type = excluded.media_type,
                series_name = excluded.series_name,
                status = excluded.status
            "#,
        )
        .bind(&group.id)
        .bind(group.source_directory.to_string_lossy().to_string())
        .bind(&group.title)
        .bind(group.year.map(|y| y as i32))
        .bind(group.tmdb_id)
        .bind(group.media_type.to_string())
        .bind(&group.series_name)
        .bind(group.status.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a title group by ID
    pub async fn get_title_group(
        &self,
        id: &str,
    ) -> Result<Option<riply_core::TitleGroup>, anyhow::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, source_directory, title, year, tmdb_id, media_type, series_name, status
            FROM title_groups
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| self.row_to_title_group(&r)).transpose()
    }

    /// Get all title groups
    pub async fn get_all_title_groups(&self) -> Result<Vec<riply_core::TitleGroup>, anyhow::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_directory, title, year, tmdb_id, media_type, series_name, status
            FROM title_groups
            ORDER BY title
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut groups = Vec::new();
        for row in rows {
            groups.push(self.row_to_title_group(&row)?);
        }
        Ok(groups)
    }

    fn row_to_title_group(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<riply_core::TitleGroup, anyhow::Error> {
        use riply_core::TitleGroupStatus;

        Ok(riply_core::TitleGroup {
            id: row.try_get("id")?,
            source_directory: std::path::PathBuf::from(
                row.try_get::<String, _>("source_directory")?,
            ),
            title: row.try_get("title")?,
            year: row.try_get::<Option<i32>, _>("year")?.map(|y| y as u16),
            tmdb_id: row.try_get("tmdb_id")?,
            media_type: self.parse_media_type(row.try_get::<String, _>("media_type")?),
            series_name: row.try_get("series_name")?,
            status: row
                .try_get::<String, _>("status")
                .map(|s| match s.as_str() {
                    "Matched" => TitleGroupStatus::Matched,
                    "Confirmed" => TitleGroupStatus::Confirmed,
                    "Moved" => TitleGroupStatus::Moved,
                    _ => TitleGroupStatus::New,
                })?,
        })
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
                title_group_id, genres, runtime, content_rating,
                overview, cast_list,
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

    /// Find all files that share the same hash (duplicates).
    /// Returns a vector of groups, where each group contains files with the same hash.
    pub async fn find_duplicates(&self) -> Result<Vec<Vec<riply_core::FileRecord>>, anyhow::Error> {
        info!("Finding duplicate files");

        let rows = sqlx::query(
            r#"
            SELECT
                id, path, filename, size, duration, hash, status,
                suggested_title, suggested_year, confidence,
                canonical_title, canonical_year, tmdb_id,
                content_type, media_type, series_name,
                season_number, episode_number, episode_title,
                source, subtitle_text, duplicate_path,
                title_group_id, genres, runtime, content_rating,
                overview, cast_list,
                reviewed, created_at, updated_at
            FROM files
            ORDER BY hash
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        // Group by hash
        let mut hash_groups: std::collections::HashMap<String, Vec<riply_core::FileRecord>> =
            std::collections::HashMap::new();
        for row in &rows {
            let file = self.row_to_file_record(row)?;
            hash_groups.entry(file.hash().to_string()).or_default().push(file);
        }

        // Only keep groups with more than one file (actual duplicates)
        let duplicates: Vec<_> = hash_groups
            .into_values()
            .filter(|group| group.len() > 1)
            .collect();

        info!(
            duplicate_groups = duplicates.len(),
            "Found duplicate groups"
        );
        Ok(duplicates)
    }

    /// Mark files as duplicates by setting their duplicate_path field.
    /// The first file in each group is kept as the "original", the rest are marked.
    pub async fn mark_duplicates(
        &self,
        duplicate_groups: &[Vec<riply_core::FileRecord>],
    ) -> Result<usize, anyhow::Error> {
        info!("Marking duplicate files");
        let mut marked = 0;

        for group in duplicate_groups {
            if group.len() < 2 {
                continue;
            }
            // First file is the original, mark the rest
            for duplicate in &group[1..] {
                sqlx::query(
                    r#"
                    UPDATE files SET
                        duplicate_path = ?,
                        updated_at = ?
                    WHERE id = ?
                    "#,
                )
                .bind(duplicate.path().to_string_lossy().to_string())
                .bind(&duplicate.updated_at())
                .bind(&duplicate.id())
                .execute(&self.pool)
                .await?;
                marked += 1;
            }
        }

        info!(marked = marked, "Marked duplicate files");
        Ok(marked)
    }

    /// Backup the database to a destination file using SQLite's VACUUM INTO.
    /// The backup is a consistent snapshot even if the DB is in use (WAL mode).
    pub async fn backup(&self, destination: &Path) -> Result<(), anyhow::Error> {
        info!(destination = %destination.display(), "Backing up database");

        if let Some(parent) = destination.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let dest_str = destination.to_string_lossy().to_string();
        let sql = format!("VACUUM INTO '{}'", dest_str.replace('\'', "''"));
        sqlx::query(&sql).execute(&self.pool).await?;

        info!(destination = %destination.display(), "Database backup complete");
        Ok(())
    }

    /// Restore the database from a backup file by replacing the current database.
    /// The pool is kept open: the backup is attached and each table/index is
    /// recreated from its original DDL, then populated from the backup data.
    pub async fn restore(&self, source: &Path) -> Result<(), anyhow::Error> {
        info!(source = %source.display(), "Restoring database from backup");

        if !source.exists() {
            anyhow::bail!("Backup file not found: {}", source.display());
        }

        let src_str = source.to_string_lossy().to_string();

        // Use a dedicated connection so ATTACH/DETACH and the copy transaction
        // all run on the same connection. DETACH must run outside the
        // transaction — an open read transaction on the attached DB (from the
        // INSERT...SELECT) makes SQLite refuse to detach with "database is
        // locked".
        let mut conn = self.pool.acquire().await?;

        let attach = format!(
            "ATTACH DATABASE '{}' AS backup_db",
            src_str.replace('\'', "''")
        );
        sqlx::query(&attach).execute(&mut *conn).await?;

        // Copy schema and data inside a transaction for atomicity.
        let mut tx = conn.begin().await?;

        // Drop existing user tables so the backup becomes the source of truth.
        let tables = sqlx::query(
            "SELECT name FROM main.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&mut *tx)
        .await?;
        for row in &tables {
            let name: String = row.get("name");
            sqlx::query(&format!("DROP TABLE IF EXISTS main.\"{}\"", name))
                .execute(&mut *tx)
                .await?;
        }

        // Recreate tables from the backup's DDL.
        let backup_tables = sqlx::query(
            "SELECT name, sql FROM backup_db.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_all(&mut *tx)
        .await?;
        for row in &backup_tables {
            let name: String = row.get("name");
            let ddl: String = row.get("sql");
            sqlx::query(&ddl).execute(&mut *tx).await?;
            sqlx::query(&format!(
                "INSERT INTO main.\"{}\" SELECT * FROM backup_db.\"{}\"",
                name, name
            ))
            .execute(&mut *tx)
            .await?;
        }

        // Recreate indexes from the backup's DDL. Unqualified DDL from the
        // attached backup creates the index in the main schema.
        let backup_indexes = sqlx::query(
            "SELECT name, sql FROM backup_db.sqlite_master WHERE type='index' AND sql IS NOT NULL",
        )
        .fetch_all(&mut *tx)
        .await?;
        for row in &backup_indexes {
            let name: String = row.get("name");
            let ddl: String = row.get("sql");
            sqlx::query(&format!("DROP INDEX IF EXISTS main.\"{}\"", name))
                .execute(&mut *tx)
                .await?;
            sqlx::query(&ddl).execute(&mut *tx).await?;
        }

        tx.commit().await?;

        // Detach after the transaction has released its read lock on backup_db.
        sqlx::query("DETACH DATABASE backup_db")
            .execute(&mut *conn)
            .await?;

        info!(source = %source.display(), "Database restored");
        Ok(())
    }

    /// Get the database file path.
    pub fn get_db_path(&self) -> &str {
        &self.db_path
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
        assert_eq!(files[0].filename(), "movie.mkv");
    }

    #[tokio::test]
    async fn test_null_optional_fields_round_trip() {
        let db = Database::new(":memory:").await.unwrap();
        db.migrate().await.unwrap();

        let file = riply_core::FileRecord::new(
            "test123".to_string(),
            std::path::PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash123".to_string(),
        );

        db.save_file(&file).await.unwrap();

        let files = db.get_all_files().await.unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].title_group_id().is_none());
        assert!(files[0].duplicate_path().is_none());

        db.update_file(&files[0]).await.unwrap();
    }

    #[tokio::test]
    async fn test_backup_restore_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let backup_path = dir.path().join("backup.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();

        let file = riply_core::FileRecord::new(
            "backup-test".to_string(),
            std::path::PathBuf::from("/test/backup.mkv"),
            "backup.mkv".to_string(),
            "hash-backup".to_string(),
        );
        db.save_file(&file).await.unwrap();

        // Backup to a separate file
        db.backup(&backup_path).await.unwrap();
        assert!(backup_path.exists());

        // Mutate the live DB so restore has something to undo
        db.delete_file("backup-test").await.unwrap();
        let files = db.get_all_files().await.unwrap();
        assert_eq!(files.len(), 0);

        // Restore from backup
        db.restore(&backup_path).await.unwrap();

        let files = db.get_all_files().await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id(), "backup-test");
        assert_eq!(files[0].filename(), "backup.mkv");
    }

    #[tokio::test]
    async fn test_enrichment_fields_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("test.db").to_str().unwrap())
            .await
            .unwrap();
        db.migrate().await.unwrap();

        let mut file = riply_core::FileRecord::new(
            "enriched".to_string(),
            std::path::PathBuf::from("/test/enriched.mkv"),
            "enriched.mkv".to_string(),
            "hash-enriched".to_string(),
        );
        *file.genres_mut() =Some(vec!["Drama".to_string(), "Crime".to_string()]);
        *file.runtime_mut() =Some(139);
        *file.content_rating_mut() =Some("R".to_string());
        *file.overview_mut() =Some("An insomniac office worker.".to_string());
        *file.cast_list_mut() =Some(vec!["Brad Pitt".to_string(), "Edward Norton".to_string()]);
        db.save_file(&file).await.unwrap();

        let files = db.get_all_files().await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].genres().as_deref(),
            Some(&["Drama".to_string(), "Crime".to_string()][..])
        );
        assert_eq!(*files[0].runtime(), Some(139));
        assert_eq!(files[0].content_rating().as_deref(), Some("R"));
        assert_eq!(
            files[0].overview().as_deref(),
            Some("An insomniac office worker.")
        );
        assert_eq!(
            files[0].cast_list().as_deref(),
            Some(&["Brad Pitt".to_string(), "Edward Norton".to_string()][..])
        );
    }
}

// ── Folder operations (ADR-026: Folder-Based Pipeline) ─────────────────────────

/// Save a folder record.
/// If a folder with the same ID already exists, updates all fields.
pub async fn save_folder(pool: &sqlx::SqlitePool, folder: &riply_core::FolderRecord) -> Result<(), anyhow::Error> {
    info!(folder = %folder.name, "Saving folder record");

    let extra_files_json = json_encode_list(&Some(folder.extra_files.clone()));

    sqlx::query(
        r#"
        INSERT INTO folders (id, path, name, suggested_name, suggested_year,
                             confidence, tmdb_id, media_type, file_count,
                             main_file, extra_files, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            path = excluded.path,
            name = excluded.name,
            suggested_name = excluded.suggested_name,
            suggested_year = excluded.suggested_year,
            confidence = excluded.confidence,
            tmdb_id = excluded.tmdb_id,
            media_type = excluded.media_type,
            file_count = excluded.file_count,
            main_file = excluded.main_file,
            extra_files = excluded.extra_files,
            status = excluded.status,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&folder.id)
    .bind(folder.path.to_string_lossy().to_string())
    .bind(&folder.name)
    .bind(&folder.suggested_name)
    .bind(folder.suggested_year)
    .bind(folder.confidence)
    .bind(folder.tmdb_id)
    .bind(folder.media_type.to_string())
    .bind(folder.file_count as i64)
    .bind(&folder.main_file)
    .bind(extra_files_json)
    .bind(folder.status.to_string())
    .bind(&folder.created_at)
    .bind(&folder.updated_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get all folder records.
pub async fn get_all_folders(pool: &sqlx::SqlitePool) -> Result<Vec<riply_core::FolderRecord>, anyhow::Error> {
    let rows = sqlx::query(
        r#"SELECT id, path, name, suggested_name, suggested_year, confidence,
                  tmdb_id, media_type, file_count, main_file, extra_files,
                  status, created_at, updated_at
           FROM folders ORDER BY name"#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| folder_row_to_record(&row))
        .collect()
}

/// Get a single folder by ID.
pub async fn get_folder_by_id(pool: &sqlx::SqlitePool, id: &str) -> Result<Option<riply_core::FolderRecord>, anyhow::Error> {
    let row = sqlx::query(
        r#"SELECT id, path, name, suggested_name, suggested_year, confidence,
                  tmdb_id, media_type, file_count, main_file, extra_files,
                  status, created_at, updated_at
           FROM folders WHERE id = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(Some(folder_row_to_record(&r)?)),
        None => Ok(None),
    }
}

/// Get folders by status.
pub async fn get_folders_by_status(pool: &sqlx::SqlitePool, status: &str) -> Result<Vec<riply_core::FolderRecord>, anyhow::Error> {
    let rows = sqlx::query(
        r#"SELECT id, path, name, suggested_name, suggested_year, confidence,
                  tmdb_id, media_type, file_count, main_file, extra_files,
                  status, created_at, updated_at
           FROM folders WHERE status = ? ORDER BY name"#,
    )
    .bind(status)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| folder_row_to_record(&row))
        .collect()
}

/// Count folders by status.
pub async fn count_folders_by_status(pool: &sqlx::SqlitePool) -> Result<Vec<(String, i64)>, anyhow::Error> {
    let rows = sqlx::query(
        r#"SELECT status, COUNT(*) FROM folders GROUP BY status"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let status: String = row.get("status");
            let count: i64 = row.get("COUNT(*)");
            (status, count)
        })
        .collect())
}

/// Update a folder's status.
pub async fn update_folder_status(pool: &sqlx::SqlitePool, id: &str, status: &str) -> Result<(), anyhow::Error> {
    use chrono::Utc;
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE folders SET status = ?, updated_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a folder by ID.
pub async fn delete_folder(pool: &sqlx::SqlitePool, id: &str) -> Result<(), anyhow::Error> {
    sqlx::query("DELETE FROM folders WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Clear all folders.
pub async fn clear_folders(pool: &sqlx::SqlitePool) -> Result<(), anyhow::Error> {
    sqlx::query("DELETE FROM folders").execute(pool).await?;
    Ok(())
}

/// Convert a database row to a FolderRecord.
fn folder_row_to_record(row: &sqlx::sqlite::SqliteRow) -> Result<riply_core::FolderRecord, anyhow::Error> {
    use std::path::PathBuf;

    let id: String = row.get("id");
    let path_str: String = row.get("path");
    let name: String = row.get("name");
    let suggested_name: Option<String> = row.get("suggested_name");
    let suggested_year: Option<i32> = row.get("suggested_year");
    let confidence: Option<f64> = row.get("confidence");
    let tmdb_id: Option<i32> = row.get("tmdb_id");
    let media_type_str: String = row.get("media_type");
    let file_count: i32 = row.get("file_count");
    let main_file: Option<String> = row.get("main_file");
    let extra_files_raw: Option<String> = row.get("extra_files");
    let status_str: String = row.get("status");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");

    let media_type = match media_type_str.as_str() {
        "Tv" | "TV Show" => riply_core::MediaType::Tv,
        _ => riply_core::MediaType::Movie,
    };

    let status = match status_str.as_str() {
        "Scanned" => riply_core::FolderStatus::Scanned,
        "Confident" => riply_core::FolderStatus::Confident,
        "Uncertain" => riply_core::FolderStatus::Uncertain,
        "Unknown" => riply_core::FolderStatus::Unknown,
        "Moved" => riply_core::FolderStatus::Moved,
        "Quarantined" => riply_core::FolderStatus::Quarantined,
        _ => riply_core::FolderStatus::Scanned,
    };

    let extra_files: Vec<String> = extra_files_raw
        .and_then(|s: String| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let path = PathBuf::from(path_str);

    Ok(riply_core::FolderRecord {
        id,
        path,
        name,
        suggested_name,
        suggested_year: suggested_year.map(|y| y as u32),
        confidence,
        tmdb_id,
        media_type,
        file_count: file_count as usize,
        main_file,
        extra_files,
        status,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod folder_tests {
    use super::*;
    use riply_core::FolderRecord;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_save_and_get_folder() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        let pool = &db.pool;

        let folder = FolderRecord::new(
            "folder-1".to_string(),
            PathBuf::from("/test/Inception (2010)"),
            "Inception (2010)".to_string(),
            3,
        );

        save_folder(pool, &folder).await.unwrap();

        let folders = get_all_folders(pool).await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "Inception (2010)");
        assert_eq!(folders[0].file_count, 3);
        assert_eq!(folders[0].status, riply_core::FolderStatus::Scanned);
    }

    #[tokio::test]
    async fn test_get_folder_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        let pool = &db.pool;

        let folder = FolderRecord::new(
            "folder-2".to_string(),
            PathBuf::from("/test/Matrix"),
            "Matrix".to_string(),
            1,
        );
        save_folder(pool, &folder).await.unwrap();

        let result = get_folder_by_id(pool, "folder-2").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Matrix");

        let none_result = get_folder_by_id(pool, "nonexistent").await.unwrap();
        assert!(none_result.is_none());
    }

    #[tokio::test]
    async fn test_update_folder_status() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        let pool = &db.pool;

        let folder = FolderRecord::new(
            "folder-3".to_string(),
            PathBuf::from("/test/Folder"),
            "Folder".to_string(),
            2,
        );
        save_folder(pool, &folder).await.unwrap();

        update_folder_status(pool, "folder-3", "Confident").await.unwrap();

        let folders = get_folders_by_status(pool, "Confident").await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].status, riply_core::FolderStatus::Confident);
    }

    #[tokio::test]
    async fn test_count_folders_by_status() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        let pool = &db.pool;

        let f1 = FolderRecord::new("fc1".to_string(), PathBuf::from("/test/A"), "A".to_string(), 1);
        let f2 = FolderRecord::new("fc2".to_string(), PathBuf::from("/test/B"), "B".to_string(), 1);
        save_folder(pool, &f1).await.unwrap();
        save_folder(pool, &f2).await.unwrap();

        update_folder_status(pool, "fc1", "Confident").await.unwrap();
        update_folder_status(pool, "fc2", "Unknown").await.unwrap();

        let counts = count_folders_by_status(pool).await.unwrap();
        let counts_map: std::collections::HashMap<_, _> = counts.into_iter().collect();
        // Only Confident and Unknown should appear (Scanned has 0 folders after updates)
        assert!(!counts_map.contains_key("Scanned"));
        assert_eq!(counts_map.get("Confident"), Some(&1));
        assert_eq!(counts_map.get("Unknown"), Some(&1));
    }

    #[tokio::test]
    async fn test_delete_folder() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        let pool = &db.pool;

        let folder = FolderRecord::new(
            "folder-del".to_string(),
            PathBuf::from("/test/ToDelete"),
            "ToDelete".to_string(),
            1,
        );
        save_folder(pool, &folder).await.unwrap();

        delete_folder(pool, "folder-del").await.unwrap();

        let folders = get_all_folders(pool).await.unwrap();
        assert_eq!(folders.len(), 0);
    }

    #[tokio::test]
    async fn test_folder_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        let pool = &db.pool;

        let files = vec![
            ("main.mkv".to_string(), 8000000000),
            ("extra.mkv".to_string(), 500000000),
        ];
        let folder = FolderRecord::with_files(
            "folder-files".to_string(),
            PathBuf::from("/test/WithFiles"),
            "WithFiles".to_string(),
            files,
        );

        save_folder(pool, &folder).await.unwrap();

        let result = get_folder_by_id(pool, "folder-files").await.unwrap().unwrap();
        assert_eq!(result.main_file, Some("main.mkv".to_string()));
        assert_eq!(result.extra_files, vec!["extra.mkv".to_string()]);
    }

    #[tokio::test]
    async fn test_save_folder_preserves_tmdb_match() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        let pool = &db.pool;

        let mut folder = FolderRecord::new(
            "folder-tmdb".to_string(),
            PathBuf::from("/test/Disc"),
            "Disc".to_string(),
            2,
        );
        folder.apply_tmdb_match("Inception".to_string(), Some(2010), 27205, riply_core::MediaType::Movie);
        save_folder(pool, &folder).await.unwrap();

        let result = get_folder_by_id(pool, "folder-tmdb").await.unwrap().unwrap();
        assert_eq!(result.suggested_name, Some("Inception".to_string()));
        assert_eq!(result.suggested_year, Some(2010));
        assert_eq!(result.tmdb_id, Some(27205));
        assert_eq!(result.confidence, Some(0.95));
        assert_eq!(result.status, riply_core::FolderStatus::Confident);
    }
}

// ── FolderStore trait impl for Database (ADR-026) ──────────────────────────────

/// Implement FolderStore for Database so app services can use it
#[async_trait::async_trait]
impl riply_core::FolderStore for Database {
    async fn save_folder(&self, folder: &riply_core::FolderRecord) -> Result<(), riply_core::FolderStoreError> {
        save_folder(&self.pool, folder).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }

    async fn get_all_folders(&self) -> Result<Vec<riply_core::FolderRecord>, riply_core::FolderStoreError> {
        get_all_folders(&self.pool).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }

    async fn get_folder_by_id(&self, id: &str) -> Result<Option<riply_core::FolderRecord>, riply_core::FolderStoreError> {
        get_folder_by_id(&self.pool, id).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }

    async fn get_folders_by_status(&self, status: &str) -> Result<Vec<riply_core::FolderRecord>, riply_core::FolderStoreError> {
        get_folders_by_status(&self.pool, status).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }

    async fn count_folders_by_status(&self) -> Result<Vec<(String, i64)>, riply_core::FolderStoreError> {
        count_folders_by_status(&self.pool).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }

    async fn update_folder_status(&self, id: &str, status: &str) -> Result<(), riply_core::FolderStoreError> {
        update_folder_status(&self.pool, id, status).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }

    async fn delete_folder(&self, id: &str) -> Result<(), riply_core::FolderStoreError> {
        delete_folder(&self.pool, id).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }

    async fn clear_folders(&self) -> Result<(), riply_core::FolderStoreError> {
        clear_folders(&self.pool).await.map_err(|e| riply_core::FolderStoreError::DatabaseError(e.to_string()))
    }
}
