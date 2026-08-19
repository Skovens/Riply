// ADR-001: Workspace Crate Architecture — core is the domain layer with zero infra deps
// ADR-004: Error Handling Strategy — thiserror for domain errors in core
// ADR-026: Folder-Based Pipeline — new folder_record module for Organizer

pub mod file_record;
pub mod folder_record;
pub mod folder_store;
pub mod media_type;
pub mod metadata;
pub mod name_cleaner;
pub mod pipeline;
pub mod ripping;
pub mod title_group;

pub use file_record::{content_type_to_str, ContentType, EnrichmentRecord, FileRecord, MetadataRecord, ScanRecord};
pub use folder_record::{FolderRecord, FolderStatus};
pub use folder_store::{FolderStore, FolderStoreError};
pub use media_type::MediaType;
pub use metadata::{MetadataEnrichment, MetadataError, MetadataMatch, MetadataSearcher};
pub use name_cleaner::NameCleaner;
pub use pipeline::PipelineStatus;
pub use ripping::{DriveInfo, DriveState, RipBackend, RipError, RipEvent, RipHandle, RipSession};
pub use title_group::{TitleGroup, TitleGroupStatus};
