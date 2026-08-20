// ADR-026: Folder-Based Pipeline — FolderStore trait for Organizer persistence

use thiserror::Error;

use crate::FolderRecord;

/// Errors that can occur during folder store operations.
#[derive(Debug, Error)]
pub enum FolderStoreError {
    #[error("Folder not found: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Database error: {0}")]
    DatabaseError(String),
}

/// Trait for folder persistence operations.
///
/// Implemented by infra adapters, consumed by app services.
/// This decouples the OrganizerService from direct database access,
/// allowing the app crate to depend only on core (not infra).
#[async_trait::async_trait]
pub trait FolderStore: Send + Sync {
    /// Save a folder record. Upserts by ID.
    async fn save_folder(&self, folder: &FolderRecord) -> Result<(), FolderStoreError>;

    /// Get all folder records, ordered by name.
    async fn get_all_folders(&self) -> Result<Vec<FolderRecord>, FolderStoreError>;

    /// Get a single folder by ID.
    async fn get_folder_by_id(&self, id: &str) -> Result<Option<FolderRecord>, FolderStoreError>;

    /// Get folders by status.
    async fn get_folders_by_status(&self, status: &str) -> Result<Vec<FolderRecord>, FolderStoreError>;

    /// Count folders grouped by status.
    async fn count_folders_by_status(&self) -> Result<Vec<(String, i64)>, FolderStoreError>;

    /// Update a folder's status.
    async fn update_folder_status(&self, id: &str, status: &str) -> Result<(), FolderStoreError>;

    /// Delete a folder by ID.
    async fn delete_folder(&self, id: &str) -> Result<(), FolderStoreError>;

    /// Clear all folders.
    async fn clear_folders(&self) -> Result<(), FolderStoreError>;
}
