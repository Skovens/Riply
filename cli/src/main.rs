use clap::{Parser, Subcommand};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

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
    Review,
    /// Move confirmed files to organized library
    Move {
        /// Movies destination directory
        #[arg(short, long)]
        movies_dir: Option<String>,
        /// TV Shows destination directory
        #[arg(short, long)]
        tv_dir: Option<String>,
        /// Enable YOLO mode (move without review)
        #[arg(long)]
        yolo: bool,
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

    info!("Riply starting");
    info!("Config file: {}", cli.config);

    // TODO: Load configuration and initialize services
    // For now, just print a message
    println!("Configuration loaded from: {}", cli.config);

    // Execute command
    match cli.command {
        Command::Scan { directory } => {
            info!("Scanning directory: {}", directory);
            println!("Scanning directory: {}", directory);
            println!("Found 0 files");
        }
        Command::Analyze => {
            info!("Analyzing files");
            println!("Analyzing scanned files...");
            println!("Matched: 0");
            println!("Needs review: 0");
            println!("Failed: 0");
        }
        Command::Review => {
            info!("Reviewing files");
            println!("Files needing review: 0");
        }
        Command::Move { movies_dir, tv_dir, yolo } => {
            info!("Moving files");
            let movies_dir = movies_dir.unwrap_or_else(|| "movies".to_string());
            let tv_dir = tv_dir.unwrap_or_else(|| "tv".to_string());
            println!("Moving files to organized library:");
            println!("  Movies: {}", movies_dir);
            println!("  TV Shows: {}", tv_dir);
            println!("  YOLO mode: {}", yolo);
            println!("  Moved: 0");
            println!("  Skipped: 0");
            println!("  Errors: 0");
        }
        Command::Rip { rip_command } => {
            match rip_command {
                RipCommand::List => {
                    info!("Listing drives");
                    println!("Available drives: 0");
                }
                RipCommand::Start { index } => {
                    info!("Starting rip on drive: {}", index);
                    println!("Starting rip on drive: {}", index);
                }
                RipCommand::Cancel { index } => {
                    info!("Cancelling rip on drive: {}", index);
                    println!("Cancelling rip on drive: {}", index);
                }
                RipCommand::Eject { index } => {
                    info!("Ejecting disc from drive: {}", index);
                    println!("Ejecting disc from drive: {}", index);
                }
            }
        }
        Command::Status => {
            info!("Getting status");
            println!("Riply Status:");
            println!("  Pipeline status: Idle");
            println!("  Files scanned: 0");
            println!("  Files matched: 0");
            println!("  Files need review: 0");
            println!("  Files failed: 0");
            println!("  Files confirmed: 0");
            println!("  Files moved: 0");
            println!("  Drives: 0");
            println!("  Active rips: 0");
            println!("  Active moves: 0");
        }
    }

    info!("Riply finished");
    Ok(())
}