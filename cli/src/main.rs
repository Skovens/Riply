use clap::{Parser, Subcommand};
use commands::{analyze_files, move_files, review_files, scan_directory, show_status};
use infra::MakeMkvClient;
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
            match rip_command {
                RipCommand::List => list_drives().await?,
                RipCommand::Start { index } => start_rip(index).await?,
                RipCommand::Cancel { index } => cancel_rip(index).await?,
                RipCommand::Eject { index } => eject_disc(index).await?,
            }
        }
        Command::Status => {
            show_status().await?;
        }
    }

    info!("Riply finished");
    Ok(())
}

/// List available MakeMKV drives
async fn list_drives() -> Result<(), anyhow::Error> {
    info!("Listing MakeMKV drives");
    
    let client = MakeMkvClient::new();
    let drives = client.detect_drives().await?;
    
    if drives.is_empty() {
        println!("No drives found");
        return Ok(());
    }
    
    println!("Available drives:");
    for drive in &drives {
        println!("  [{}] {} - {}", drive.index, drive.name, drive.state);
        if let Some(name) = &drive.disc_name {
            println!("      Disc: {}", name);
        }
    }
    
    Ok(())
}

/// Start ripping on a drive
async fn start_rip(index: usize) -> Result<(), anyhow::Error> {
    info!("Starting rip on drive: {}", index);
    
    let client = MakeMkvClient::new();
    let mut session = client.start_rip(index, ".").await?;
    
    println!("Ripping started on drive {}", index);
    println!("This may take several minutes...");
    
    // Monitor rip progress
    loop {
        let progress = session.progress();
        println!("\rProgress: {:.1}%", progress * 100.0);
        
        if session.is_complete() {
            break;
        }
        
        // Simulate progress check (in real implementation, this would be async)
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    
    println!("\nRip complete");
    Ok(())
}

/// Cancel ripping on a drive
async fn cancel_rip(_index: usize) -> Result<(), anyhow::Error> {
    info!("Cancelling rip (placeholder)");
    println!("Rip cancellation not yet implemented");
    Ok(())
}

/// Eject disc from a drive
async fn eject_disc(index: usize) -> Result<(), anyhow::Error> {
    info!("Ejecting disc from drive: {}", index);
    println!("Disc eject not yet implemented");
    Ok(())
}