use std::fmt;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

// ── Drive types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveInfo {
    pub index: usize,
    pub name: String,
    pub disc_name: Option<String>,
    pub derived_name: Option<String>,
    pub device_path: Option<String>,
    pub state: DriveState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// ── Rip events ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RipEvent {
    Progress { current: u64, total: u64, max: u64 },
    Message(String),
    Complete,
    Error(String),
}

// ── Error type ────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum RipError {
    #[error("No ripping backend available. Install makemkvcon or Flatpak package.")]
    NotAvailable,

    #[error("No disc in drive {0}")]
    NoDisc(usize),

    #[error("Drive {0} not found")]
    DriveNotFound(usize),

    #[error("Destination is not writable: {0}")]
    DestinationNotWritable(String),

    #[error("Destination parent directory does not exist: {0}")]
    DestinationParentMissing(String),

    #[error("Insufficient disk space: {0}")]
    InsufficientSpace(String),

    #[error("MakeMKV license not accepted. Accept the license in MakeMKV settings.")]
    LicenseNotAccepted,

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Traits ────────────────────────────────────────────────────

/// Sync trait for ripping backends. Fully dyn-compatible.
///
/// `detect_drives()` is a blocking operation (runs a subprocess).
/// `start_rip()` spawns a process and returns a `RipHandle` for
/// streaming events asynchronously.
pub trait RipBackend: Send + Sync {
    /// Detect available optical drives.
    fn detect_drives(&self) -> Result<Vec<DriveInfo>, RipError>;

    /// Start ripping all titles from a drive.
    fn start_rip(&self, drive: usize, dest: &str) -> Result<RipHandle, RipError>;
}

/// Async trait for streaming rip events from a running rip.
///
/// Uses `Pin<Box<dyn Future>>` for dyn compatibility since this trait
/// is stored as `Box<dyn RipSession>` inside `RipHandle`.
pub trait RipSession: Send + Sync {
    /// Wait for the next event. Returns `None` when the rip is done.
    fn next_event(&mut self) -> Pin<Box<dyn Future<Output = Option<RipEvent>> + Send + '_>>;

    /// Cancel the rip by killing the underlying process.
    fn cancel(&mut self) -> Pin<Box<dyn Future<Output = Result<(), RipError>> + Send + '_>>;
}

// ── RipHandle ─────────────────────────────────────────────────

/// Concrete wrapper around a `RipSession`. Consumers call
/// `next_event()` and `cancel()` without dealing with trait objects.
pub struct RipHandle {
    inner: Box<dyn RipSession>,
}

impl RipHandle {
    pub fn new(session: Box<dyn RipSession>) -> Self {
        Self { inner: session }
    }

    /// Wait for the next event from the rip process.
    pub async fn next_event(&mut self) -> Option<RipEvent> {
        self.inner.next_event().await
    }

    /// Cancel the rip by killing the process.
    pub async fn cancel(&mut self) -> Result<(), RipError> {
        self.inner.cancel().await
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drive_state_display() {
        assert_eq!(format!("{}", DriveState::Empty), "Empty");
        assert_eq!(format!("{}", DriveState::Loading), "Loading");
        assert_eq!(format!("{}", DriveState::Ready), "Ready");
        assert_eq!(format!("{}", DriveState::Ripping), "Ripping");
        assert_eq!(format!("{}", DriveState::Complete), "Complete");
        assert_eq!(format!("{}", DriveState::Error), "Error");
    }

    #[test]
    fn test_drive_state_equality() {
        assert_eq!(DriveState::Empty, DriveState::Empty);
        assert_ne!(DriveState::Empty, DriveState::Ready);
    }

    #[test]
    fn test_rip_event_clone() {
        let event = RipEvent::Progress {
            current: 50,
            total: 100,
            max: 100,
        };
        let cloned = event.clone();
        match cloned {
            RipEvent::Progress {
                current,
                total,
                max,
            } => {
                assert_eq!(current, 50);
                assert_eq!(total, 100);
                assert_eq!(max, 100);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_rip_error_display() {
        let err = RipError::NotAvailable;
        assert!(err.to_string().contains("backend"));

        let err = RipError::DriveNotFound(3);
        assert!(err.to_string().contains("3"));

        let err = RipError::NoDisc(1);
        assert!(err.to_string().contains("1"));

        let err = RipError::Backend("test".to_string());
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn test_rip_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "nope");
        let rip_err: RipError = io_err.into();
        assert!(matches!(rip_err, RipError::Io(_)));
    }
}
