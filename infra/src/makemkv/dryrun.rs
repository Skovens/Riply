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
                derived_name: Some("Test Movie (2024)".to_string()),
                device_path: Some("/dev/sr0".to_string()),
                state: DriveState::Ready,
            },
            DriveInfo {
                index: 1,
                name: "Fake Blu-ray Drive".to_string(),
                disc_name: None,
                derived_name: None,
                device_path: Some("/dev/sr1".to_string()),
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

    /// Integration test: Full ripping and media naming workflow
    ///
    /// This test simulates:
    /// 1. Detecting drives with DryRunBackend
    /// 2. Starting a rip from a fake DVD drive
    /// 3. Creating simulated rip output files
    /// 4. Scanning the output directory
    /// 5. Analyzing files for metadata and naming
    #[tokio::test]
    async fn test_integration_ripping_and_media_naming() {
        use std::fs;

        use app::analyzer::AnalyzerService;
        use app::scanner::ScannerService;
        use tempfile::tempdir;

        // Step 1: Detect drives using DryRunBackend
        let backend = DryRunBackend::new();
        let drives = backend.detect_drives().unwrap();

        assert_eq!(drives.len(), 2, "Should detect 2 fake drives");
        assert_eq!(
            drives[0].state,
            DriveState::Ready,
            "DVD drive should be ready"
        );
        assert_eq!(
            drives[1].state,
            DriveState::Empty,
            "Blu-ray drive should be empty"
        );

        // Verify the DVD drive has a disc name
        assert!(
            drives[0].disc_name.is_some(),
            "DVD drive should have a disc name"
        );
        let disc_name = drives[0].disc_name.as_ref().unwrap();
        assert_eq!(disc_name, "Test Movie (2024)");

        // Step 2: Start ripping
        let temp_dir = tempdir().unwrap();
        let rip_dest = temp_dir.path().join("rips");
        fs::create_dir_all(&rip_dest).unwrap();

        let mut handle = backend.start_rip(0, rip_dest.to_str().unwrap()).unwrap();

        // Collect rip events
        let mut events = Vec::new();
        while let Some(event) = handle.next_event().await {
            events.push(event);
        }

        // Verify rip completed successfully
        assert!(
            events.iter().any(|e| matches!(e, RipEvent::Complete)),
            "Rip should complete"
        );

        // Step 3: Create simulated rip output files
        // Simulate the structure that MakeMKV would create
        let movie_dir = rip_dest.join("Test Movie (2024)");
        fs::create_dir_all(&movie_dir).unwrap();

        // Create a title file (main movie)
        fs::write(movie_dir.join("title_t00.mkv"), "simulated movie content").unwrap();

        // Create a bonus feature
        fs::write(movie_dir.join("title_t01.mkv"), "simulated bonus content").unwrap();

        // Step 4: Scan the rip output directory
        let scanner = ScannerService::new();
        let scan_result = scanner
            .scan_directory(&movie_dir, None)
            .await
            .expect("Scan should succeed");

        assert!(
            !scan_result.records.is_empty(),
            "Should find media files in rip output"
        );
        assert_eq!(
            scan_result.records.len(),
            2,
            "Should find 2 media files (title + bonus)"
        );

        // Step 5: Analyze files for metadata
        for record in &scan_result.records {
            let analyzer = AnalyzerService::new(None);
            let analyzed = analyzer
                .analyze_file(record)
                .await
                .expect("Analysis should succeed");

            // Verify the file was analyzed
            assert!(analyzed.filename.contains(".mkv"));
            assert!(!analyzed.hash.is_empty(), "Hash should be populated");
        }
    }

    /// Integration test: TV show ripping and naming
    ///
    /// Tests the workflow for ripping a TV series DVD with multiple episodes
    #[tokio::test]
    async fn test_integration_tv_ripping_and_naming() {
        use std::fs;

        use app::scanner::ScannerService;
        use tempfile::tempdir;

        let backend = DryRunBackend::new();

        // Simulate ripping a TV series
        let temp_dir = tempdir().unwrap();
        let rip_dest = temp_dir.path().join("rips");
        fs::create_dir_all(&rip_dest).unwrap();

        let mut handle = backend.start_rip(0, rip_dest.to_str().unwrap()).unwrap();

        // Wait for rip to complete
        while handle.next_event().await.is_some() {}

        // Create TV series directory structure
        let tv_dir = rip_dest.join("Breaking Bad S01");
        fs::create_dir_all(&tv_dir).unwrap();

        // Create episode files with proper naming convention
        fs::write(tv_dir.join("Breaking.Bad.S01E01.mkv"), "episode 1 content").unwrap();
        fs::write(tv_dir.join("Breaking.Bad.S01E02.mkv"), "episode 2 content").unwrap();
        fs::write(tv_dir.join("Breaking.Bad.S01E03.mkv"), "episode 3 content").unwrap();

        // Scan the TV series directory
        let scanner = ScannerService::new();
        let scan_result = scanner
            .scan_directory(&tv_dir, None)
            .await
            .expect("Scan should succeed");

        assert_eq!(scan_result.records.len(), 3, "Should find 3 episode files");

        // Verify episode files have correct naming
        let filenames: Vec<&str> = scan_result
            .records
            .iter()
            .map(|r| r.filename.as_str())
            .collect();

        assert!(
            filenames.iter().any(|f| f.contains("S01E01")),
            "Should have episode 1"
        );
        assert!(
            filenames.iter().any(|f| f.contains("S01E02")),
            "Should have episode 2"
        );
        assert!(
            filenames.iter().any(|f| f.contains("S01E03")),
            "Should have episode 3"
        );
    }

    /// Integration test: Drive state transitions
    ///
    /// Tests that rip events are correctly reported during the ripping workflow
    #[tokio::test]
    async fn test_integration_rip_events() {
        use std::fs;

        use tempfile::tempdir;

        let backend = DryRunBackend::new();

        let temp_dir = tempdir().unwrap();
        let rip_dest = temp_dir.path().join("rips");
        fs::create_dir_all(&rip_dest).unwrap();

        let mut handle = backend.start_rip(0, rip_dest.to_str().unwrap()).unwrap();

        // Collect events and verify state transitions
        let mut rip_started = false;
        let mut rip_complete = false;
        let mut progress_events = Vec::new();

        while let Some(event) = handle.next_event().await {
            match event {
                RipEvent::Message(msg) => {
                    assert!(msg.contains("Starting dry-run rip"));
                    rip_started = true;
                }
                RipEvent::Progress { current, total, .. } => {
                    assert!(current <= total);
                    progress_events.push((current, total));
                }
                RipEvent::Complete => {
                    rip_complete = true;
                }
                RipEvent::Error(_) => {
                    panic!("Should not encounter errors in dry-run rip");
                }
            }
        }

        assert!(rip_started, "Rip should have started");
        assert!(rip_complete, "Rip should have completed");
        assert!(
            progress_events.len() >= 4,
            "Should have at least 4 progress events"
        );
    }

    /// Integration test: Multiple drives
    ///
    /// Tests that multiple drives can be detected and ripped from independently
    #[tokio::test]
    async fn test_integration_multiple_drives() {
        use tempfile::tempdir;

        let backend = DryRunBackend::new();
        let drives = backend.detect_drives().unwrap();

        // Should detect both drives
        assert_eq!(drives.len(), 2);

        // Rip from the DVD drive (index 0)
        let temp_dir1 = tempdir().unwrap();
        let mut handle1 = backend
            .start_rip(0, temp_dir1.path().to_str().unwrap())
            .unwrap();

        // Rip from the Blu-ray drive (index 1)
        let temp_dir2 = tempdir().unwrap();
        let mut handle2 = backend
            .start_rip(1, temp_dir2.path().to_str().unwrap())
            .unwrap();

        // Both rips should complete independently
        let events1 = collect_events(&mut handle1).await;
        let events2 = collect_events(&mut handle2).await;

        assert!(
            events1.iter().any(|e| matches!(e, RipEvent::Complete)),
            "DVD rip should complete"
        );
        assert!(
            events2.iter().any(|e| matches!(e, RipEvent::Complete)),
            "Blu-ray rip should complete"
        );
    }

    /// Helper function to collect all events from a rip handle
    async fn collect_events(handle: &mut RipHandle) -> Vec<RipEvent> {
        let mut events = Vec::new();
        while let Some(event) = handle.next_event().await {
            events.push(event);
        }
        events
    }
}
