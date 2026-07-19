use clap::{Parser, Subcommand};
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

    // Load configuration
    let config = infra::Config::load(&cli.config)?;

    // Initialize services
    let database = infra::Database::new(&config.db_path).await?;
    let http_client = infra::HttpClient::new();
    let make_mkv_client = infra::MakeMkvClient::new();
    let file_system = infra::FileSystem::new();
    let app = app::App::new(
        database,
        http_client,
        make_mkv_client,
        file_system,
        config.clone(),
    );

    // Execute command
    match cli.command {
        Command::Scan { directory } => {
            info!("Scanning directory: {}", directory);
            app.scanner().scan_directory(&directory).await?;
        }
        Command::Analyze => {
            info!("Analyzing files");
            app.analyzer().analyze_all().await?;
        }
        Command::Review => {
            info!("Reviewing files");
            app.review().start().await?;
        }
        Command::Move { movies_dir, tv_dir, yolo } => {
            info!("Moving files");
            let movies_dir = movies_dir.unwrap_or_else(|| "movies".to_string());
            let tv_dir = tv_dir.unwrap_or_else(|| "tv".to_string());
            app.mover().move_files(&movies_dir, &tv_dir, yolo).await?;
        }
        Command::Rip { rip_command } => {
            match rip_command {
                RipCommand::List => {
                    info!("Listing drives");
                    let drives = app.ripper().list_drives().await?;
                    println!("{:#?}", drives);
                }
                RipCommand::Start { index } => {
                    info!("Starting rip on drive: {}", index);
                    app.ripper().start_rip(index).await?;
                }
                RipCommand::Cancel { index } => {
                    info!("Cancelling rip on drive: {}", index);
                    app.ripper().cancel_rip(index).await?;
                }
                RipCommand::Eject { index } => {
                    info!("Ejecting disc from drive: {}", index);
                    app.ripper().eject_disc(index).await?;
                }
            }
        }
        Command::Status => {
            info!("Getting status");
            let status = app.status().get().await?;
            println!("{:#?}", status);
        }
    }

    info!("Riply finished");
    Ok(())
}