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
        // TODO: Implement drive detection
        Ok(vec![])
    }

    /// Start a rip session
    pub async fn start_rip(&self, _drive_index: usize, _destination: &str) -> Result<RipSession, anyhow::Error> {
        info!("Starting rip session");
        // TODO: Implement rip session
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

/// A rip session
pub struct RipSession;

impl RipSession {
    /// Get current progress
    pub fn progress(&self) -> f64 {
        0.0
    }

    /// Check if session is complete
    pub fn is_complete(&self) -> bool {
        false
    }
}