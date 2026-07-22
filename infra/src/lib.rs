// Infrastructure layer - I/O heavy adapters (database, HTTP, filesystem, MakeMKV)
pub mod config;
pub mod db;
pub mod fs;
pub mod http;
pub mod makemkv;

pub use config::Config;
pub use db::Database;
pub use fs::FileSystem;
pub use http::HttpClient;
pub use makemkv::MakeMkvClient;
