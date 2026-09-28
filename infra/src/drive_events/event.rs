/// Standardized drive event type used by all sources.
///
/// All adapters (udev, inotify, D-Bus) convert their native events
/// into this type so the rest of the system never sees raw events.

/// Represents a disc insertion or ejection event from any source.
#[derive(Debug, Clone)]
pub enum DriveEvent {
    /// A disc was inserted into a drive.
    Inserted {
        /// MakeMKV drive index (0, 1, 2, ...)
        drive_index: usize,
        /// OS device path (e.g., `/dev/sr0`)
        device_path: String,
        /// Optional disc name from MakeMKV
        disc_name: Option<String>,
    },
    /// A disc was ejected from a drive.
    Ejected {
        /// MakeMKV drive index (0, 1, 2, ...)
        drive_index: usize,
        /// OS device path (e.g., `/dev/sr0`)
        device_path: String,
    },
}

impl DriveEvent {
    /// Returns the drive index for this event.
    pub fn drive_index(&self) -> usize {
        match self {
            Self::Inserted { drive_index, .. } | Self::Ejected { drive_index, .. } => *drive_index,
        }
    }

    /// Returns the device path for this event.
    pub fn device_path(&self) -> &str {
        match self {
            Self::Inserted { device_path, .. } | Self::Ejected { device_path, .. } => device_path,
        }
    }

    /// Returns true if this is an insertion event.
    pub fn is_inserted(&self) -> bool {
        matches!(self, Self::Inserted { .. })
    }

    /// Returns true if this is an ejection event.
    pub fn is_ejected(&self) -> bool {
        matches!(self, Self::Ejected { .. })
    }
}
