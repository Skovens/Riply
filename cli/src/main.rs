use clap::{Parser, Subcommand};
use commands::{analyze_files, move_files, review_files, rip_command, scan_directory, show_status, RipCommand};
use infra::{Config, Database};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod commands;

/// Riply - Rip, manage, move media files
#[derive(Parser, Debug)]
#[command(name = "riply", version, about)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Scan a directory for media files
    Scan {
        /// Directory to scan
        directory: String,
    },
    /// Analyze scanned files
    Analyze,
    /// Review files that need manual confirmation
    Review {
        /// Enable YOLO mode (move without review)
        #[arg(long)]
        yolo: bool,
    },
    /// Move confirmed files to organized library
    Move,
    /// Manage disc ripping
    Rip {
        /// Subcommand for rip operations
        #[command(subcommand)]
        rip_command: RipCommand,
    },
    /// Show current pipeline status
    Status,
}

#[derive(Subcommand, Debug)]
enum RipCommand {
    /// List available drives
    List,
    /// Start ripping on a drive
    Start {
        /// Drive index to rip
        index: usize,
    },
    /// Cancel ripping on a drive
    Cancel {
        /// Drive index to cancel
        index: usize,
    },
    /// Eject disc from a drive
    Eject {
        /// Drive index to eject
        index: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if cli.verbose { Level::DEBUG } else { Level::INFO })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

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
    info!("Config file: {}", cli.config);
    info!("Database: {}", config.database_path);

    // Initialize database
    let db = Database::new(&config.database_path).await?;
    db.migrate().await?;

    // Execute command
    match cli.command {
        Command::Scan { directory } => {
            scan_directory(&directory, &config).await?;
        }
        Command::Analyze => {
            analyze_files(&db).await?;
        }
        Command::Review { yolo } => {
            review_files(yolo, &db).await?;
        }
        Command::Move => {
            move_files(&db, &config).await?;
        }
        Command::Rip { rip_command } => {
            rip_command(rip_command).await?;
        }
        Command::Status => {
            show_status(&db).await?;
        }
    }

    info!("Riply finished");
    Ok(())
}