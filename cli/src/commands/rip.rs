use clap::Subcommand;
use riply_core::{RipBackend, RipError};
use tracing::info;

/// Subcommands for rip operations
#[derive(Subcommand, Debug)]
pub enum RipCommand {
    /// List available drives
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Start ripping on a drive
    Start {
        /// Drive index to rip
        index: usize,
        /// Destination directory (default: rip_output_directory from config)
        #[arg(short, long)]
        destination: Option<String>,
    },
    /// Eject disc from a drive
    Eject {
        /// Drive index to eject
        index: usize,
    },
}

/// Execute rip commands
pub async fn rip_command(command: RipCommand, config: &infra::Config) -> Result<(), anyhow::Error> {
    match command {
        RipCommand::List { json } => list_drives(json).await,
        RipCommand::Start { index, destination } => {
            let dest = match destination {
                Some(d) => d,
                None => {
                    config
                        .validate_for_ripping()
                        .map_err(|e| anyhow::anyhow!(e))?;
                    config
                        .rip_output_directory
                        .clone()
                        .unwrap_or_else(|| ".".to_string())
                }
            };
            start_rip(index, &dest).await
        }
        RipCommand::Eject { index } => eject_disc(index).await,
    }
}

/// Helper to create a backend, printing a helpful message if not available.
fn get_backend() -> Result<Box<dyn RipBackend>, anyhow::Error> {
    match infra::MakemkvBackend::auto_detect() {
        Ok(backend) => Ok(Box::new(backend)),
        Err(RipError::NotAvailable) => {
            println!("No ripping backend found.\n");
            println!("Run `riply setup` to install MakeMKV via Flatpak.");
            Err(anyhow::anyhow!("No ripping backend available"))
        }
        Err(e) => Err(e.into()),
    }
}

/// List available MakeMKV drives
async fn list_drives(json: bool) -> Result<(), anyhow::Error> {
    info!("Listing drives");
    let backend = get_backend()?;

    let drives = backend.detect_drives()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&drives)?);
        return Ok(());
    }

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
async fn start_rip(index: usize, destination: &str) -> Result<(), anyhow::Error> {
    info!("Starting rip on drive: {}", index);
    let backend = get_backend()?;

    // Look up disc name and create subdirectory if applicable
    let mut dest = destination.to_string();
    let drives = backend.detect_drives()?;
    if let Some(drive) = drives.iter().find(|d| d.index == index) {
        if let Some(ref disc_name) = drive.disc_name {
            let (title, _year) = infra::makemkv::parser::parse_disc_name(disc_name);
            if !title.is_empty() {
                dest = format!("{}/{}", dest, title);
            }
        }
    }

    let mut handle = backend.start_rip(index, &dest)?;

    println!("Ripping started on drive {} → {}", index, dest);
    println!("Press Ctrl+C to cancel");

    while let Some(event) = handle.next_event().await {
        match event {
            riply_core::RipEvent::Progress {
                current,
                total,
                max,
            } => {
                let pct = if max > 0 {
                    (current as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                print!("\rProgress: {:.1}%", pct);
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
            riply_core::RipEvent::Complete => {
                println!("\nRip complete!");
                break;
            }
            riply_core::RipEvent::Error(e) => {
                println!("\nError: {}", e);
                return Err(anyhow::anyhow!(e));
            }
            riply_core::RipEvent::Message(msg) => {
                println!("{}", msg);
            }
        }
    }

    Ok(())
}

/// Eject disc from a drive
async fn eject_disc(index: usize) -> Result<(), anyhow::Error> {
    info!("Ejecting disc from drive: {}", index);

    let backend = get_backend()?;
    let drives = backend.detect_drives()?;
    let device_path = drives
        .iter()
        .find(|d| d.index == index)
        .and_then(|d| d.device_path.clone())
        .ok_or_else(|| anyhow::anyhow!("Drive {} not found or has no device path", index))?;

    infra::eject_disc(&device_path)?;
    println!("Ejected disc from drive {} ({})", index, device_path);
    Ok(())
}
