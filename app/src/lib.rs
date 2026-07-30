// Application layer - orchestration of core logic with infrastructure
pub mod analyzer;
pub mod ffprobe;
pub mod jobs;
pub mod mover;
pub mod scanner;

pub use analyzer::AnalyzerService;
pub use ffprobe::extract_metadata;
pub use jobs::{
    CancellationToken, JobInfo, JobKind, JobProgress, JobScheduler, JobStatus, ProgressHandle,
};
pub use mover::MoverService;
pub use scanner::{ScanResult, ScannerService};
