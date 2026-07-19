use std::fmt;
use tracing::info;

/// MakeMKV client adapter - handles CLI interaction and drive management
pub struct MakeMkvClient;

impl MakeMkvClient {
    /// Create a new MakeMKV client
    pub fn new() -> Self {
        info!("Initializing MakeMKV client");
        MakeMkvClient
    }

    /// Detect available drives
    pub async fn detect_drives(&self) -> Result<Vec<DriveInfo>, anyhow::Error> {
        info!("Detecting drives");
        // TODO: Implement drive detection using MakeMKV CLI
        Ok(vec![])
    }

    /// Start a rip session
    pub async fn start_rip(&self, _drive_index: usize, _destination: &str) -> Result<RipSession, anyhow::Error> {
        info!("Starting rip session");
        // TODO: Implement rip session using MakeMKV CLI
        Ok(RipSession)
    }

    /// Cancel a rip session
    pub async fn cancel_rip(&self, _session: &mut RipSession) -> Result<(), anyhow::Error> {
        info!("Cancelling rip session");
        // TODO: Implement rip cancellation
        Ok(())
    }
}

/// Information about a detected drive
#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub index: usize,
    pub name: String,
    pub disc_name: Option<String>,
    pub state: DriveState,
}

/// Drive state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveState {
    Empty,
    Loading,
    Ready,
    Ripping,
    Complete,
    Error,
}

impl fmt::Display for DriveState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveState::Empty => write!(f, "Empty"),
            DriveState::Loading => write!(f, "Loading"),
            DriveState::Ready => write!(f, "Ready"),
            DriveState::Ripping => write!(f, "Ripping"),
            DriveState::Complete => write!(f, "Complete"),
            DriveState::Error => write!(f, "Error"),
        }
    }
}

/// A rip session
pub struct RipSession;

impl RipSession {
    /// Get current progress (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        // TODO: Implement actual progress tracking
        0.0
    }

    /// Check if session is complete
    pub fn is_complete(&self) -> bool {
        // TODO: Implement actual completion check
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drive_state_display() {
        assert_eq!(format!("{}", DriveState::Empty), "Empty");
        assert_eq!(format!("{}", DriveState::Ready), "Ready");
        assert_eq!(format!("{}", DriveState::Ripping), "Ripping");
        assert_eq!(format!("{}", DriveState::Complete), "Complete");
        assert_eq!(format!("{}", DriveState::Error), "Error");
    }

    #[tokio::test]
    async fn test_detect_drives_empty() {
        let client = MakeMkvClient::new();
        let drives = client.detect_drives().await.unwrap();
        assert!(drives.is_empty());
    }
}