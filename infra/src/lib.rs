// Infrastructure layer - I/O heavy adapters (database, HTTP, MakeMKV)
pub mod config;
pub mod db;
pub mod http;
pub mod makemkv;

pub use config::{Config, RippingConfig};
pub use db::Database;
pub use http::HttpClient;
pub use makemkv::eject_disc;
pub use makemkv::setup::{
    check_installation, install_makemkv, DetectionMethod, InstallationStatus, MissingComponent,
};
pub use makemkv::{DryRunBackend, MakemkvBackend, MakemkvSession};
