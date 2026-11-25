package rip

import (
	"context"
)

// DiscInfo holds information about a disc in a drive.
type DiscInfo struct {
	Title string
	// Hash is a deterministic fingerprint of the disc's title metadata
	// (SHA-256 of the sorted TINFO code/value pairs). Identically-named
	// discs with different contents produce different hashes. Empty when
	// the info output carried no TINFO metadata.
	Hash string
}

// RipProgress represents the progress of a rip operation.
type RipProgress struct {
	Title      int     // completed .mkv file count at emit time
	Percent    float64 // byte-driven when total known; PRGV fallback otherwise
	SpeedMBPS  float64 // MB/s, smoothed over a ~10s window (EMA)
	TotalBytes int64   // bytes written so far across all .mkv files (0 when unknown)
	EstimatedTotal int64 // estimated total bytes to write (0 when unknown); drives ETA
	Phase      string  // human-readable MakeMKV phase
	Saved      int     // MSG:5004 saved count
	Failed     int     // MSG:5004 failed count
	Current    int     // PRGV current value
	Max        int     // PRGV max value
	Status     string  // "ripping", "phase", "subphase", "title", "saving", "save_result", "makemkv_complete", "complete", "error", "prgv", "title_saved"
	RipPhase   string  // "scanning" | "saving" | "complete" — which stage of the rip operation
	Message    string
	TitleIndex int // MSG:5017: 1-based index of the just-saved title (0 when unknown)
	TitleTotal int // MSG:5017: total titles in this rip (0 when unknown)
}

// Ripper is the interface that MakeMKV implementations must satisfy.
// Drive addressing uses the OS device path (e.g. /dev/sr0); the
// implementation maps it to its own drive numbering (MakeMKV enumerates
// drives itself and the order need not match /dev/srN).
type Ripper interface {
	// GetDiscInfo retrieves information about the disc in the drive at devPath.
	GetDiscInfo(ctx context.Context, devPath string) (DiscInfo, error)

	// Rip starts a rip operation on the drive at devPath and returns a channel
	// that emits progress updates. The context can be used to cancel the operation.
	// totalBytes is the estimated total bytes to write (media capacity × rip factor);
	// 0 means "unknown" and the ripper falls back to a sysfs read, then to PRGV.
	Rip(ctx context.Context, devPath string, outDir string, totalBytes int64) <-chan RipProgress

	// Eject ejects the disc from the drive at devPath.
	Eject(ctx context.Context, devPath string) error

	// Version returns the MakeMKV version string.
	Version() string

	// IsAvailable checks if MakeMKV is installed and accessible.
	IsAvailable() bool
}

// MinLengthSetter is implemented by rippers that can change the minimum
// title length at runtime; the API asserts this optional interface rather
// than widening Ripper, so simple test doubles stay simple.
type MinLengthSetter interface {
	SetMinTitleLength(seconds int)
}
