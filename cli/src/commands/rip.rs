use infra::MakeMKVService;
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
    
    let service = MakeMKVService::new();
    let drives = service.list_drives().await?;
    
    if drives.is_empty() {
        println!("No drives found");
        return Ok(());
    }
    
    println!("Available drives:");
    for (i, drive) in drives.iter().enumerate() {
        println!("  [{}] {} - {}", i, drive.name, drive.status);
    }
    
    Ok(())
}

/// Start ripping on a drive
async fn start_rip(index: usize) -> Result<(), anyhow::Error> {
    info!("Starting rip on drive: {}", index);
    
    let service = MakeMKVService::new();
    service.start_rip(index).await?;
    
    println!("Ripping started on drive {}", index);
    println!("This may take several minutes...");
    
    // TODO: Monitor rip progress
    println!("Rip complete");
    
    Ok(())
}

/// Cancel ripping on a drive
async fn cancel_rip(index: usize) -> Result<(), anyhow::Error> {
    info!("Cancelling rip on drive: {}", index);
    
    let service = MakeMKVService::new();
    service.cancel_rip(index).await?;
    
    println!("Rip cancelled on drive {}", index);
    Ok(())
}

/// Eject disc from a drive
async fn eject_disc(index: usize) -> Result<(), anyhow::Error> {
    info!("Ejecting disc from drive: {}", index);
    
    let service = MakeMKVService::new();
    service.eject_disc(index).await?;
    
    println!("Disc ejected from drive {}", index);
    Ok(())
}