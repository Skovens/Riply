//! Event-driven drive detection system.
//!
//! Replaces continuous polling with event-driven detection from multiple
//! sources (udev, inotify, D-Bus). Uses the Adapter pattern and Chain
//! of Responsibility to select the best available source at startup.
//!
//! Architecture:
//! - `event.rs`: Standardized DriveEvent type
//! - `adapter.rs`: DriveEventSource trait (Adapter interface)
//! - `chain.rs`: DriveEventChain (Chain of Responsibility)

pub mod event;
pub mod adapter;
pub mod chain;

// Re-export for convenience
pub use event::DriveEvent;
pub use adapter::DriveEventSource;
pub use chain::DriveEventChain;
pub use chain::SelectedSource;
