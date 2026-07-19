// Infrastructure layer - I/O heavy adapters (database, HTTP, filesystem, MakeMKV)
pub mod db;
pub mod http;
pub mod makemkv;
pub mod fs;
pub mod config;

pub use db::Database;
pub use http::HttpClient;
pub use makemkv::MakeMkvClient;
pub use fs::FileSystem;
pub use config::Config;