use infra::MakeMkvClient;
use tracing::info;

/// Execute rip commands
pub async fn rip_command(command: RipCommand) -> Result<(), anyhow::Error> {
    match command {
        RipCommand::List => list_drives().await,
        RipCommand::Start { index } => start_rip(index).await,
        RipCommand::Cancel { index } => cancel_rip(index).await,
        RipCommand::Eject { index } => eject_disc(index).await,
    }
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