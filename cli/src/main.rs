// ADR-001: Workspace Crate Architecture — cli is the interface layer
// ADR-005: Async Runtime — #[tokio::main]
// ADR-007: CLI Framework — Clap derive macros
// ADR-008: Logging — tracing + tracing-subscriber

use clap::{Parser, Subcommand, ValueEnum};
use commands::{
    analyze_files, backup_database, duplicate::find_duplicates, library::scan_library, move_files,
    restore_database, review_files, rip_command as do_rip, run_setup, scan_directory, show_status,
    watch::watch_directory, RipCommand, SetupArgs,
};
use infra::{Config, Database};
use riply_core::{MediaType, MetadataSearcher};
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

mod commands;

/// Riply - Rip, manage, move media files
#[derive(Parser, Debug)]
#[command(name = "riply", version, about)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Enable verbose logging (sets log level to DEBUG)
    #[arg(short, long)]
    verbose: bool,

    /// Log output format
    #[arg(long, value_enum, default_value_t = LogFormat::Compact)]
    log_format: LogFormat,

    /// Subcommand to execute
    #[command(subcommand)]
    command: Command,
}

#[derive(ValueEnum, Clone, Debug)]
enum LogFormat {
    /// Compact, single-line log output (default)
    Compact,
    /// Multi-line, colorized log output
    Pretty,
    /// Structured JSON log output
    Json,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Scan a directory for media files
    Scan {
        /// Directory to scan
        directory: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Analyze scanned files
    Analyze {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Review files that need manual confirmation
    Review {
        /// Enable YOLO mode (move without review)
        #[arg(long)]
        yolo: bool,
    },
    /// Move confirmed files to organized library
    Move {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Manage disc ripping
    Rip {
        /// Subcommand for rip operations
        #[command(subcommand)]
        rip_command: RipCommand,
    },
    /// Start the web API server
    Serve {
        /// Bind address (overrides config)
        #[arg(long)]
        host: Option<String>,
        /// Bind port (overrides config)
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Show current pipeline status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Find duplicate files
    Duplicate {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Scan an organized media library
    Library {
        /// Directory to scan
        directory: String,
        /// Media type (movie or tv)
        #[arg(long, default_value = "movie")]
        media_type: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Watch a directory for new files
    Watch {
        /// Directory to watch
        directory: String,
        /// Media type (movie or tv)
        #[arg(long, default_value = "movie")]
        media_type: String,
    },
    /// Backup the database to a file
    Backup {
        /// Destination backup file path (default: riply_backup_<timestamp>.db)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
    /// Restore the database from a backup file
    Restore {
        /// Source backup file to restore from
        source: std::path::PathBuf,
    },
    /// Set up MakeMKV installation
    Setup(SetupArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cli.verbose {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        }
    });

    match cli.log_format {
        LogFormat::Json => fmt().json().with_env_filter(env_filter).init(),
        LogFormat::Pretty => fmt().pretty().with_env_filter(env_filter).init(),
        LogFormat::Compact => fmt().compact().with_env_filter(env_filter).init(),
    }

    // Load configuration
    let config = match Config::load(&cli.config) {
        Ok(config) => config,
        Err(_) => {
            info!("Configuration file not found, creating default");
            Config::create_default(&cli.config)?;
            Config::load(&cli.config)?
        }
    };

    info!("Riply starting");
    info!(config = %cli.config, "Config file");
    info!(path = %config.database_path, "Database");

    // Initialize database
    let db = Database::new(&config.database_path).await?;
    db.migrate().await?;

    // Execute command
    match cli.command {
        Command::Scan { directory, json } => {
            let searcher: Option<Box<dyn MetadataSearcher>> =
                if config.tmdb_api_key.is_some() || config.omdb_api_key.is_some() {
                    let client = infra::HttpClient::new(
                        config.tmdb_api_key.clone(),
                        config.omdb_api_key.clone(),
                    );
                    Some(Box::new(client))
                } else {
                    None
                };
            scan_directory(&directory, &config, searcher.as_deref(), json).await?;
        }
        Command::Analyze { json } => {
            analyze_files(&db, &config, json).await?;
        }
        Command::Review { yolo } => {
            review_files(yolo, &db).await?;
        }
        Command::Move { json } => {
            move_files(&db, &config, json).await?;
        }
        Command::Rip { rip_command } => {
            do_rip(rip_command, &config).await?;
        }
        Command::Serve { host, port } => {
            commands::serve(db, config, Some(cli.config), host, port).await?;
        }
        Command::Status { json } => {
            show_status(&db, json).await?;
        }
        Command::Duplicate { json } => {
            find_duplicates(&db, &config, json).await?;
        }
        Command::Library {
            directory,
            media_type,
            json,
        } => {
            let media_type = match media_type.as_str() {
                "tv" => MediaType::Tv,
                _ => MediaType::Movie,
            };
            scan_library(&directory, &config, media_type, json).await?;
        }
        Command::Watch {
            directory,
            media_type,
        } => {
            let media_type = match media_type.as_str() {
                "tv" => MediaType::Tv,
                _ => MediaType::Movie,
            };
            watch_directory(&directory, &config, media_type).await?;
        }
        Command::Backup { output } => {
            backup_database(&db, output).await?;
        }
        Command::Restore { source } => {
            restore_database(&db, source).await?;
        }
        Command::Setup(args) => {
            run_setup(args)?;
        }
    }

    info!("Riply finished");
    Ok(())
}
