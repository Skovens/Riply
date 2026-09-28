// ADR-001: Workspace Crate Architecture — infra is the I/O adapter layer
// ADR-002: Database Technology — SQLite + SQLx
// ADR-009: Disc Ripping Abstraction — trait-based backend (MakemkvBackend, DryRunBackend)

// Infrastructure layer - I/O heavy adapters (database, HTTP, MakeMKV)
pub mod config;
pub mod db;
pub mod drive_events;
pub mod healthchecks;
pub mod http;
pub mod makemkv;
pub mod rate_limiter;

pub use config::{Config, RippingConfig};
pub use db::Database;
pub use drive_events::DriveEventChain;
pub use http::HttpClient;
pub use makemkv::eject_disc;
pub use makemkv::setup::{
    check_installation, check_installation_with_dest, install_makemkv, DetectionMethod,
    InstallationStatus, MissingComponent, SandboxIssue,
};
pub use makemkv::{DryRunBackend, MakemkvBackend, MakemkvSession};
