// Commands module - CLI subcommand implementations
pub mod analyze;
pub mod backup;
pub mod duplicate;
pub mod library;
pub mod mover;
pub mod review;
pub mod rip;
pub mod scan;
pub mod serve;
pub mod setup;
pub mod status;
pub mod watch;

pub use analyze::analyze_files;
pub use backup::{backup_database, restore_database};
pub use duplicate::find_duplicates;
pub use library::scan_library;
pub use mover::move_files;
pub use review::review_files;
pub use rip::{rip_command, RipCommand};
pub use scan::scan_directory;
pub use serve::serve;
pub use setup::{run_setup, SetupArgs};
pub use status::show_status;
pub use watch::watch_directory;
