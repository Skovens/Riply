use std::future::Future;
use std::pin::Pin;

use riply_core::{DriveInfo, DriveState, RipBackend, RipError, RipEvent, RipHandle, RipSession};

/// Dry-run backend for testing and CI.
/// Returns fake drives and simulates rip progress.
#[derive(Default)]
pub struct DryRunBackend;

impl DryRunBackend {
    pub fn new() -> Self {
        Self
    }
}

impl RipBackend for DryRunBackend {
    fn detect_drives(&self) -> Result<Vec<DriveInfo>, RipError> {
        Ok(vec![
            DriveInfo {
                index: 0,
                name: "Fake DVD Drive".to_string(),
                disc_name: Some("Test Movie (2024)".to_string()),
                state: DriveState::Ready,
            },
            DriveInfo {
                index: 1,
                name: "Fake Blu-ray Drive".to_string(),
                disc_name: None,
                state: DriveState::Empty,
            },
        ])
    }

    fn start_rip(&self, drive: usize, _dest: &str) -> Result<RipHandle, RipError> {
        let session = DryRunSession::new(drive);
        Ok(RipHandle::new(Box::new(session)))
    }
}

struct DryRunSession {
    drive: usize,
    step: u32,
    cancelled: bool,
}

impl DryRunSession {
    fn new(drive: usize) -> Self {
        Self {
            drive,
            step: 0,
            cancelled: false,
        }
    }
}

impl RipSession for DryRunSession {
    fn next_event(&mut self) -> Pin<Box<dyn Future<Output = Option<RipEvent>> + Send + '_>> {
        Box::pin(async move {
            if self.cancelled {
                return None;
            }

            // Simulate a few progress events, then complete
            self.step += 1;
            match self.step {
                1 => Some(RipEvent::Message(format!(
                    "Starting dry-run rip on drive {}",
                    self.drive
                ))),
                2 => Some(RipEvent::Progress {
                    current: 25,
                    total: 100,
                    max: 100,
                }),
                3 => Some(RipEvent::Progress {
                    current: 50,
                    total: 100,
                    max: 100,
                }),
                4 => Some(RipEvent::Progress {
                    current: 75,
                    total: 100,
                    max: 100,
                }),
                5 => Some(RipEvent::Progress {
                    current: 100,
                    total: 100,
                    max: 100,
                }),
                6 => Some(RipEvent::Complete),
                _ => None,
            }
        })
    }

    fn cancel(&mut self) -> Pin<Box<dyn Future<Output = Result<(), RipError>> + Send + '_>> {
        Box::pin(async move {
            self.cancelled = true;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dryrun_detect_drives() {
        let backend = DryRunBackend::new();
        let drives = backend.detect_drives().unwrap();
        assert_eq!(drives.len(), 2);
        assert_eq!(drives[0].state, DriveState::Ready);
        assert_eq!(drives[1].state, DriveState::Empty);
    }

    #[tokio::test]
    async fn test_dryrun_rip_flow() {
        let backend = DryRunBackend::new();
        let mut handle = backend.start_rip(0, "/tmp/dryrun").unwrap();

        let mut events = Vec::new();
        while let Some(event) = handle.next_event().await {
            events.push(format!("{:?}", event));
        }

        assert!(events.len() >= 5); // message + 4 progress + complete
        assert!(events.last().unwrap().contains("Complete"));
    }

    #[tokio::test]
    async fn test_dryrun_cancel() {
        let backend = DryRunBackend::new();
        let mut handle = backend.start_rip(0, "/tmp/dryrun").unwrap();

        // Get first event
        let first = handle.next_event().await;
        assert!(first.is_some());

        // Cancel
        handle.cancel().await.unwrap();

        // Should return None after cancel
        let next = handle.next_event().await;
        assert!(next.is_none());
    }
}
