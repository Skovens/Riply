use clap::Subcommand;
use infra::{MakeMkvClient, MkvError};
use tracing::info;

/// Subcommands for rip operations
#[derive(Subcommand, Debug)]
pub enum RipCommand {
    /// List available drives
    List,
    /// Start ripping on a drive
    Start {
        /// Drive index to rip
        index: usize,
        /// Destination directory (default: current directory)
        #[arg(short, long, default_value = ".")]
        destination: String,
    },
    /// Cancel ripping on a drive (press Ctrl+C during a rip instead)
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

/// Execute rip commands
pub async fn rip_command(command: RipCommand) -> Result<(), anyhow::Error> {
    match command {
        RipCommand::List => list_drives().await,
        RipCommand::Start { index, destination } => start_rip(index, &destination).await,
        RipCommand::Cancel { index } => cancel_rip(index).await,
        RipCommand::Eject { index } => eject_disc(index).await,
    }
}

/// Helper to create a client, printing a helpful message if not installed.
fn get_client() -> Result<MakeMkvClient, anyhow::Error> {
    match MakeMkvClient::new(None) {
        Ok(client) => Ok(client),
        Err(MkvError::NotInstalled) => {
            println!("MakeMKV is not installed.");
            println!();
            println!("Install instructions:");
            println!("  1. Download from https://www.makemkv.com/download/");
            println!("  2. Build from source:");
            println!("     https://www.makemkv.com/forum/viewtopic.php?t=224");
            println!();
            println!("Quick build (Ubuntu/Debian):");
            println!("  sudo apt-get install build-essential pkg-config libc6-dev \\");
            println!("    libssl-dev libexpat1-dev libavcodec-dev \\");
            println!("    libgl1-mesa-dev qtbase5-dev zlib1g-dev");
            println!("  tar xf makemkv-oss-*.tar.gz && cd makemkv-oss-* && ./configure && make && sudo make install");
            println!("  tar xf makemkv-bin-*.tar.gz && cd makemkv-bin-* && make && sudo make install");
            Err(anyhow::anyhow!("MakeMKV not installed"))
        }
        Err(e) => Err(e.into()),
    }
}

/// List available MakeMKV drives
async fn list_drives() -> Result<(), anyhow::Error> {
    info!("Listing MakeMKV drives");
    let client = get_client()?;

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
async fn start_rip(index: usize, destination: &str) -> Result<(), anyhow::Error> {
    info!("Starting rip on drive: {}", index);
    let client = get_client()?;
    let mut handle = client.start_rip(index, destination).await?;

    println!("Ripping started on drive {} → {}", index, destination);
    println!("Press Ctrl+C to cancel");

    while let Some(event) = handle.next_event().await {
        match event {
            infra::RipEvent::Progress {
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
            infra::RipEvent::Complete => {
                println!("\nRip complete!");
                break;
            }
            infra::RipEvent::Error(e) => {
                println!("\nError: {}", e);
                return Err(anyhow::anyhow!(e));
            }
            infra::RipEvent::Message(msg) => {
                println!("{}", msg);
            }
        }
    }

    Ok(())
}

/// Cancel ripping on a drive
async fn cancel_rip(_index: usize) -> Result<(), anyhow::Error> {
    println!("To cancel a rip, press Ctrl+C while a rip is running.");
    println!("The rip process will be killed automatically.");
    Ok(())
}

/// Eject disc from a drive
async fn eject_disc(index: usize) -> Result<(), anyhow::Error> {
    info!("Ejecting disc from drive: {}", index);

    // Try Linux eject command
    let device = format!("/dev/sr{}", index);
    match std::process::Command::new("eject")
        .arg(&device)
        .output()
    {
        Ok(output) if output.status.success() => {
            println!("Ejected disc from drive {} ({})", index, device);
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No medium found") {
                println!("Drive {} is already empty", index);
                Ok(())
            } else if stderr.contains("not found") || stderr.contains("No such") {
                Err(anyhow::anyhow!(
                    "Device {} not found. Check that the drive index is correct.",
                    device
                ))
            } else {
                Err(anyhow::anyhow!("Eject failed: {}", stderr.trim()))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err(anyhow::anyhow!(
                    "'eject' command not found. Install with: sudo apt-get install eject"
                ))
            } else {
                Err(e.into())
            }
        }
    }
}
