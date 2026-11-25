package drive

import (
	"time"
)

// EventType represents the kind of disc event.
type EventType string

const (
	// DiscInserted signals a disc was inserted into a known drive.
	DiscInserted EventType = "inserted"
	// DiscEjected signals a disc was ejected from a known drive.
	DiscEjected EventType = "ejected"
	// DriveDiscovered signals a new drive device was detected.
	DriveDiscovered EventType = "discovered"
	// DriveRemoved signals a drive device was removed.
	DriveRemoved EventType = "removed"
)

// DiscEvent represents a disc state change detected by the drive poller.
type DiscEvent struct {
	Path      string // /dev/sr0
	EventType EventType
	Time      time.Time
	MediaSize int64 // media capacity in bytes (0 when unknown)
	SizeKnown bool  // MediaSize was probed successfully
}
