// ADR-001: Workspace Crate Architecture — core is the domain layer with zero infra deps
// ADR-004: Error Handling Strategy — thiserror for domain errors in core

pub mod file_record;
pub mod media_type;
pub mod metadata;
pub mod pipeline;
pub mod ripping;

pub use file_record::FileRecord;
pub use media_type::MediaType;
pub use metadata::{MetadataError, MetadataMatch, MetadataSearcher};
pub use pipeline::PipelineStatus;
pub use ripping::{DriveInfo, DriveState, RipBackend, RipError, RipEvent, RipHandle, RipSession};
