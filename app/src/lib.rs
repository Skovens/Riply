// ADR-001: Workspace Crate Architecture — app is the orchestration layer
// ADR-010: Job Scheduler — JobScheduler with CancellationToken and ProgressHandle
// ADR-026: Folder-Based Pipeline — OrganizerService for the Organizer window

// Application layer - orchestration of core logic with infrastructure
pub mod analyzer;
pub mod ffprobe;
pub mod jobs;
pub mod mover;
pub mod organizer;
pub mod scanner;

pub use analyzer::AnalyzerService;
pub use ffprobe::{extract_metadata, find_video_files, verify_episode_duration};
pub use jobs::{
    CancellationToken, JobInfo, JobKind, JobProgress, JobScheduler, JobStatus, ProgressHandle,
};
pub use mover::MoverService;
pub use organizer::{OrganizerService, ScanFoldersResult};
pub use scanner::{ScanResult, ScannerService};
