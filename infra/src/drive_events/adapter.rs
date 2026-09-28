/// Trait that all drive event sources must implement.
///
/// Each source (udev, inotify, D-Bus) wraps its native event format
/// into the standardized `DriveEvent` type.

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::drive_events::event::DriveEvent;

/// Trait for drive event sources.
///
/// All adapters implement this trait, making them interchangeable.
/// The system selects one adapter at startup based on availability.
pub trait DriveEventSource: Send + Sync + 'static {
    /// Human-readable name of this source (e.g., "udev", "inotify_sysfs").
    fn name(&self) -> &str;

    /// Check if this source is available on the current system.
    ///
    /// This must be non-blocking and fast (< 100ms).
    fn is_available(&self) -> bool;

    /// Start listening for events.
    ///
    /// Sends events to the channel. This is a blocking call — should be
    /// called in a spawned task. Runs until the channel is closed or
    /// the source becomes unavailable.
    ///
    /// Errors during listening should be logged, not propagated.
    fn listen(self: Arc<Self>, tx: mpsc::UnboundedSender<DriveEvent>);
}
