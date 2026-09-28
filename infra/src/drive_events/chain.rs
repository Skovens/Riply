/// Chain of responsibility for selecting a drive event source.
///
/// Sources are tried in priority order. First available source wins.
/// Selection happens at server startup.

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::drive_events::adapter::DriveEventSource;
use crate::drive_events::event::DriveEvent;

/// Result of chain selection.
pub struct SelectedSource {
    /// The selected event source (if any).
    pub source: Option<Arc<dyn DriveEventSource>>,
    /// Whether a source was successfully selected.
    pub is_available: bool,
}

/// Chain of drive event sources.
///
/// Tries sources in priority order:
/// 1. udev
/// 2. inotify on /sys/block/
/// 3. inotify on /dev/sr*
/// 4. D-Bus
pub struct DriveEventChain {
    sources: Vec<Arc<dyn DriveEventSource>>,
}

impl DriveEventChain {
    /// Create a new chain with all available sources.
    ///
    /// Sources are added in priority order.
    pub fn new() -> Self {
        let mut sources: Vec<Arc<dyn DriveEventSource>> = Vec::new();

        // Priority 1: udev (most reliable on Linux)
        sources.push(Arc::new(UdevSource));

        // Priority 2: inotify on /sys/block/
        sources.push(Arc::new(InotifySysfsSource));

        // Priority 3: inotify on /dev/sr*
        sources.push(Arc::new(InotifyDevSource));

        // Priority 4: D-Bus
        sources.push(Arc::new(DbusSource));

        Self { sources }
    }

    /// Select the first available source.
    ///
    /// Returns the selected source and whether one was found.
    pub fn select(&self) -> SelectedSource {
        for source in &self.sources {
            if source.is_available() {
                tracing::info!("Event source selected: {}", source.name());
                return SelectedSource {
                    source: Some(Arc::clone(source)),
                    is_available: true,
                };
            }
        }

        tracing::info!("No event source available — polling disabled");
        SelectedSource {
            source: None,
            is_available: false,
        }
    }

    /// Get all sources for logging/debugging.
    pub fn sources(&self) -> &[Arc<dyn DriveEventSource>] {
        &self.sources
    }
}

impl Default for DriveEventChain {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- Adapter implementations ----------

/// udev adapter (Priority 1)
struct UdevSource;

impl DriveEventSource for UdevSource {
    fn name(&self) -> &str {
        "udev"
    }

    fn is_available(&self) -> bool {
        // Check if udev runtime directory exists
        std::path::Path::new("/run/udev").exists()
            || std::path::Path::new("/dev/.udev").exists()
    }

    fn listen(self: Arc<Self>, _tx: mpsc::UnboundedSender<DriveEvent>) {
        // udev implementation — see udev.rs for details
        tracing::warn!("udev source available but not yet implemented — falling back to next source");
    }
}

/// inotify on /sys/block/ adapter (Priority 2)
struct InotifySysfsSource;

impl DriveEventSource for InotifySysfsSource {
    fn name(&self) -> &str {
        "inotify_sysfs"
    }

    fn is_available(&self) -> bool {
        std::path::Path::new("/sys/block/").is_dir()
    }

    fn listen(self: Arc<Self>, tx: mpsc::UnboundedSender<DriveEvent>) {
        Self::run_loop(tx);
    }
}

/// inotify on /dev/sr* adapter (Priority 3)
struct InotifyDevSource;

impl DriveEventSource for InotifyDevSource {
    fn name(&self) -> &str {
        "inotify_dev"
    }

    fn is_available(&self) -> bool {
        // Check if any /dev/sr* devices exist
        (0..10).any(|i| std::path::Path::new(&format!("/dev/sr{}", i)).exists())
    }

    fn listen(self: Arc<Self>, tx: mpsc::UnboundedSender<DriveEvent>) {
        Self::run_loop(tx);
    }
}

/// D-Bus adapter (Priority 4)
struct DbusSource;

impl DriveEventSource for DbusSource {
    fn name(&self) -> &str {
        "dbus"
    }

    fn is_available(&self) -> bool {
        // Check if D-Bus system bus is accessible
        std::process::Command::new("dbus-send")
            .args([
                "--system",
                "--dest=org.freedesktop.DBus",
                "--type=method_call",
                "--print-reply",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus.ListNames",
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn listen(self: Arc<Self>, _tx: mpsc::UnboundedSender<DriveEvent>) {
        // D-Bus implementation — see dbus.rs for details
        tracing::warn!("D-Bus source available but not yet implemented — falling back to next source");
    }
}

impl InotifySysfsSource {
    fn run_loop(tx: mpsc::UnboundedSender<DriveEvent>) {
        tracing::info!("inotify_sysfs listener started");
        Self::watch_sys_block(tx);
    }

    /// Watch /sys/block/ for device creation/deletion events.
    fn watch_sys_block(tx: mpsc::UnboundedSender<DriveEvent>) {
        use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};

        // Open inotify
        let inotify = match Inotify::init(InitFlags::empty()) {
            Ok(i) => i,
            Err(e) => {
                tracing::error!("Failed to initialize inotify: {}", e);
                return;
            }
        };

        // Watch /sys/block/ for file creation and deletion
        let flags = AddWatchFlags::IN_CREATE | AddWatchFlags::IN_DELETE;
        if inotify.add_watch("/sys/block/", flags).is_err() {
            tracing::error!("Failed to add inotify watch on /sys/block/");
            return;
        }

        loop {
            match inotify.read_events() {
                Ok(events) if events.is_empty() => continue,
                Ok(_) => {
                    // Scan for optical drives on any event
                    Self::scan_and_send(&tx);
                }
                Err(e) => {
                    tracing::warn!("inotify read error: {}", e);
                    // Wait briefly before retrying
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }

    /// Scan /sys/block/ for optical drives and send events.
    fn scan_and_send(tx: &mpsc::UnboundedSender<DriveEvent>) {
        // Collect optical block device names
        let optical_devs = Self::scan_optical_drives();

        if optical_devs.is_empty() {
            return;
        }

        // Run makemkvcon to get drive mapping
        let makemkv_drives = Self::run_makemkv_list();

        // Build a map: device_path -> (drive_index, disc_name)
        let mut drive_map: std::collections::HashMap<String, (usize, Option<String>)> =
            std::collections::HashMap::new();

        for md in makemkv_drives {
            if let Some(ref dp) = md.device_path {
                drive_map.insert(dp.clone(), (md.index, md.disc_name));
            }
        }

        // Send events for each optical device
        for dev_name in optical_devs {
            let dev_path = format!("/dev/{}", dev_name);
            if let Some((drive_index, disc_name)) = drive_map.get(&dev_path) {
                let event = DriveEvent::Inserted {
                    drive_index: *drive_index,
                    device_path: dev_path.clone(),
                    disc_name: disc_name.clone(),
                };
                if tx.send(event).is_err() {
                    tracing::warn!("Channel closed, stopping listener");
                    return;
                }
            }
        }
    }

    /// Scan /sys/block/ for optical drive device names.
    /// Returns a list of block device names (e.g., ["sr0", "sr1"]).
    fn scan_optical_drives() -> Vec<String> {
        let mut result = Vec::new();

        let block_dir = std::path::Path::new("/sys/block/");
        if !block_dir.is_dir() {
            return result;
        }

        let entries = match std::fs::read_dir(block_dir) {
            Ok(e) => e,
            Err(_) => return result,
        };

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();

            // Check if this is an optical drive by reading device/type
            let type_path = entry.path().join("device").join("type");
            if let Ok(contents) = std::fs::read_to_string(&type_path) {
                // Type 5 = CD/DVD (optical), 0 = disk, 7 = scanner, 8 = tape
                if contents.trim() == "5" {
                    result.push(name_str);
                }
            }
        }

        result.sort();
        result
    }

    /// Run makemkvcon -r --cache=1 info disc:9999 to get drive info.
    /// Returns a list of parsed drives.
    fn run_makemkv_list() -> Vec<crate::makemkv::parser::ParsedDrive> {
        // Try to find makemkvcon in PATH
        let output = std::process::Command::new("makemkvcon")
            .args(["-r", "--cache=1", "info", "disc:9999"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok());

        let Some(stdout) = output else {
            return Vec::new();
        };

        stdout
            .lines()
            .filter_map(|line| crate::makemkv::parser::parse_drive_line(line))
            .filter(|d| d.visible)
            .collect()
    }
}

impl InotifyDevSource {
    fn run_loop(tx: mpsc::UnboundedSender<DriveEvent>) {
        tracing::info!("inotify_dev listener started");
        Self::watch_dev(tx);
    }

    /// Watch /dev/ for sr* device node changes.
    fn watch_dev(tx: mpsc::UnboundedSender<DriveEvent>) {
        use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};

        let inotify = match Inotify::init(InitFlags::empty()) {
            Ok(i) => i,
            Err(e) => {
                tracing::error!("Failed to initialize inotify: {}", e);
                return;
            }
        };

        // Watch /dev/ for any file creation or deletion
        let flags = AddWatchFlags::IN_CREATE | AddWatchFlags::IN_DELETE;
        if inotify.add_watch("/dev/", flags).is_err() {
            tracing::error!("Failed to add inotify watch on /dev/");
            return;
        }

        loop {
            match inotify.read_events() {
                Ok(events) if events.is_empty() => continue,
                Ok(_) => {
                    // Check if any sr* devices were created/removed
                    Self::scan_and_send(&tx);
                }
                Err(e) => {
                    tracing::warn!("inotify read error: {}", e);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }

    /// Scan for sr* devices and send events.
    fn scan_and_send(tx: &mpsc::UnboundedSender<DriveEvent>) {
        // Find all sr* devices
        let sr_devs: Vec<String> = (0..10)
            .map(|i| format!("/dev/sr{}", i))
            .filter(|p| std::path::Path::new(p).exists())
            .collect();

        if sr_devs.is_empty() {
            return;
        }

        // Run makemkvcon to get drive mapping
        let makemkv_drives = InotifySysfsSource::run_makemkv_list();

        let mut drive_map: std::collections::HashMap<String, (usize, Option<String>)> =
            std::collections::HashMap::new();

        for md in makemkv_drives {
            if let Some(ref dp) = md.device_path {
                drive_map.insert(dp.clone(), (md.index, md.disc_name));
            }
        }

        for dev_path in sr_devs {
            if let Some((drive_index, disc_name)) = drive_map.get(&dev_path) {
                let event = DriveEvent::Inserted {
                    drive_index: *drive_index,
                    device_path: dev_path.clone(),
                    disc_name: disc_name.clone(),
                };
                if tx.send(event).is_err() {
                    tracing::warn!("Channel closed, stopping listener");
                    return;
                }
            }
        }
    }
}
