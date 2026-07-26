pub mod parser;

use std::fmt;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{debug, info};

// ── Error type ────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum MkvError {
    #[error("MakeMKV is not installed. Install from https://www.makemkv.com/forum/viewtopic.php?t=224")]
    NotInstalled,

    #[error("makemkvcon failed: {0}")]
    CommandFailed(String),

    #[error("Drive {0} not found")]
    DriveNotFound(usize),

    #[error("No disc in drive {0}")]
    NoDisc(usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Drive info ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct DriveInfo {
    pub index: usize,
    pub name: String,
    pub disc_name: Option<String>,
    pub state: DriveState,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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

impl From<&parser::ParsedDrive> for DriveState {
    fn from(d: &parser::ParsedDrive) -> Self {
        if d.disc_name.is_none() {
            DriveState::Empty
        } else if d.enabled {
            DriveState::Ready
        } else {
            DriveState::Error
        }
    }
}

// ── Rip events ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RipEvent {
    Progress {
        current: u64,
        total: u64,
        max: u64,
    },
    Message(String),
    Complete,
    Error(String),
}

// ── Rip handle ────────────────────────────────────────────────

/// Handle to a running rip process. Read events via `next_event()`.
/// Dropping this handle kills the child process.
pub struct RipHandle {
    child: Option<Child>,
    rx: mpsc::UnboundedReceiver<RipEvent>,
}

impl RipHandle {
    /// Wait for the next event from the rip process.
    /// Returns `None` when the process has exited and all events are consumed.
    pub async fn next_event(&mut self) -> Option<RipEvent> {
        self.rx.recv().await
    }

    /// Kill the rip process immediately.
    pub async fn cancel(&mut self) -> Result<(), MkvError> {
        if let Some(ref mut child) = self.child {
            info!("Killing makemkvcon process");
            child.kill().await.ok();
            self.child = None;
        }
        Ok(())
    }
}

impl Drop for RipHandle {
    fn drop(&mut self) {
        // kill_on_drop(true) is set when spawning, so the child is
        // automatically killed when this handle is dropped.
    }
}

// ── MakeMkvClient ─────────────────────────────────────────────

/// Client for interacting with the `makemkvcon` CLI.
#[derive(Debug)]
pub struct MakeMkvClient {
    binary_path: PathBuf,
}

impl MakeMkvClient {
    /// Create a new client. If `path` is `None`, auto-detects by
    /// checking `which makemkvcon` and `/usr/bin/makemkvcon`.
    pub fn new(path: Option<&str>) -> Result<Self, MkvError> {
        let binary_path = match path {
            Some(p) => {
                let pb = PathBuf::from(p);
                if !pb.exists() {
                    return Err(MkvError::CommandFailed(format!(
                        "makemkvcon not found at: {}",
                        p
                    )));
                }
                pb
            }
            None => Self::find_binary()?,
        };

        info!("MakeMKV client initialized: {}", binary_path.display());
        Ok(MakeMkvClient { binary_path })
    }

    /// Detect available optical drives by running:
    /// `makemkvcon -r --cache=1 info disc:9999`
    pub async fn detect_drives(&self) -> Result<Vec<DriveInfo>, MkvError> {
        info!("Detecting drives via {}", self.binary_path.display());

        let output = Command::new(&self.binary_path)
            .args(["-r", "--cache=1", "info", "disc:9999"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MkvError::CommandFailed(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut drives = Vec::new();

        for line in stdout.lines() {
            if let Some(parsed) = parser::parse_drive_line(line) {
                if parsed.visible {
                    let state = DriveState::from(&parsed);
                    drives.push(DriveInfo {
                        index: parsed.index,
                        name: parsed.drive_name,
                        disc_name: parsed.disc_name,
                        state,
                    });
                }
            }
        }

        debug!("Found {} drives", drives.len());
        Ok(drives)
    }

    /// Start ripping all titles from a drive.
    ///
    /// Spawns `makemkvcon -r --cache=1024 --noscan mkv disc:{index} all {dest}`
    /// and returns a `RipHandle` for monitoring progress.
    pub async fn start_rip(
        &self,
        drive_index: usize,
        destination: &str,
    ) -> Result<RipHandle, MkvError> {
        let dest = Path::new(destination);
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }

        info!(
            "Starting rip: drive {} → {}",
            drive_index,
            dest.display()
        );

        let mut child = Command::new(&self.binary_path)
            .args([
                "-r",
                "--cache=1024",
                "--noscan",
                "mkv",
                &format!("disc:{}", drive_index),
                "all",
                destination,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdout = child.stdout.take().ok_or_else(|| {
            MkvError::CommandFailed("Failed to capture stdout".to_string())
        })?;

        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn a reader task that parses stdout lines and sends events
        let reader_tx = tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                debug!("makemkvcon: {}", line);

                if let Some(prog) = parser::parse_progress_line(&line) {
                    let _ = reader_tx.send(RipEvent::Progress {
                        current: prog.current,
                        total: prog.total,
                        max: prog.max,
                    });
                } else if parser::is_completion_line(&line) {
                    let _ = reader_tx.send(RipEvent::Complete);
                } else if parser::is_error_message(&line) {
                    if let Some(msg) = parser::parse_message_line(&line) {
                        let _ = reader_tx.send(RipEvent::Error(msg.message));
                    }
                } else if let Some(msg) = parser::parse_message_line(&line) {
                    let _ = reader_tx.send(RipEvent::Message(msg.message));
                }
            }

            debug!("Reader task finished");
        });

        Ok(RipHandle {
            child: Some(child),
            rx,
        })
    }

    fn find_binary() -> Result<PathBuf, MkvError> {
        // Try `which makemkvcon`
        if let Ok(output) = std::process::Command::new("which").arg("makemkvcon").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }
        }

        // Try common paths
        for path in &["/usr/bin/makemkvcon", "/usr/local/bin/makemkvcon"] {
            if Path::new(path).exists() {
                return Ok(PathBuf::from(path));
            }
        }

        Err(MkvError::NotInstalled)
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

    #[test]
    fn test_drive_state_from_parsed_drive() {
        let parsed = parser::ParsedDrive {
            index: 0,
            visible: true,
            enabled: true,
            flags: 0,
            drive_name: "test".to_string(),
            disc_name: Some("movie".to_string()),
        };
        assert_eq!(DriveState::from(&parsed), DriveState::Ready);

        let no_disc = parser::ParsedDrive {
            index: parsed.index,
            visible: parsed.visible,
            enabled: parsed.enabled,
            flags: parsed.flags,
            drive_name: parsed.drive_name.clone(),
            disc_name: None,
        };
        assert_eq!(DriveState::from(&no_disc), DriveState::Empty);

        let disabled = parser::ParsedDrive {
            index: parsed.index,
            visible: parsed.visible,
            enabled: false,
            flags: parsed.flags,
            drive_name: parsed.drive_name,
            disc_name: Some("movie".to_string()),
        };
        assert_eq!(DriveState::from(&disabled), DriveState::Error);
    }

    #[test]
    fn test_client_not_installed() {
        let result = MakeMkvClient::new(Some("/nonexistent/makemkvcon"));
        assert!(result.is_err());
    }

    #[test]
    fn test_client_not_installed_auto() {
        // Should fail gracefully if makemkvcon isn't on the system
        // (CI environments won't have it)
        let result = MakeMkvClient::new(None);
        if result.is_err() {
            match result.unwrap_err() {
                MkvError::NotInstalled => {} // expected in CI
                other => panic!("Unexpected error: {}", other),
            }
        }
    }

    #[test]
    fn test_mkv_error_display() {
        let err = MkvError::NotInstalled;
        assert!(err.to_string().contains("MakeMKV"));

        let err = MkvError::DriveNotFound(3);
        assert!(err.to_string().contains("3"));

        let err = MkvError::CommandFailed("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
