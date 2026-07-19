use clap::{Parser, Subcommand};
use commands::{analyze_files, move_files, rip_command, review_files, scan_directory, show_status, RipCommand};
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
    Move {
        /// Movies destination directory
        #[arg(short, long)]
        movies_dir: Option<String>,
        /// TV Shows destination directory
        #[arg(short, long)]
        tv_dir: Option<String>,
    },
    /// Manage disc ripping
    Rip {
        /// Subcommand for rip operations
        #[command(subcommand)]
        rip_command: RipCommand,
    },
    /// Show current pipeline status
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if cli.verbose { Level::DEBUG } else { Level::INFO })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Riply starting");
    info!("Config file: {}", cli.config);

    // Execute command
    match cli.command {
        Command::Scan { directory } => {
            scan_directory(&directory).await?;
        }
        Command::Analyze => {
            analyze_files().await?;
        }
        Command::Review { yolo } => {
            review_files(yolo).await?;
        }
        Command::Move { movies_dir, tv_dir } => {
            move_files(movies_dir, tv_dir).await?;
        }
        Command::Rip { rip_command } => {
            rip_command(rip_command).await?;
        }
        Command::Status => {
            show_status().await?;
        }
    }

    info!("Riply finished");
    Ok(())
}