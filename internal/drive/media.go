package drive

import (
	"os"
	"strings"
	"sync"

	"riply/internal/logger"
)

// sysfsWarned remembers drives for which we already warned that the sysfs
// fallback (possibly stale) is in use, so the Info line is emitted at most
// once per drive instead of every 2 s poll.
var sysfsWarned sync.Map

// logSysfsFallbackStale logs one Info per drive path when the sysfs fallback
// was reached via a failed device open/ioctl — the very condition under which
// /sys attributes are stale (they only refresh on device open), so detection
// can stay wrong until the open succeeds.
func logSysfsFallbackStale(devPath, reason string) {
	if _, loaded := sysfsWarned.LoadOrStore(devPath, true); !loaded {
		logger.Info("Media probe falling back to sysfs; values may be stale until the device can be opened",
			"path", devPath, "reason", reason)
	}
}

// mediaState is the result of probing a drive for disc presence.
type mediaState int

const (
	mediaUnknown mediaState = iota // drive could not report a usable state
	mediaAbsent
	mediaPresent
)

// mediaStatusFromSysfs infers disc presence from sysfs attributes. The kernel
// only refreshes these when the device is opened, so they can be stale; this is
// only a fallback for when the ioctl probe is unavailable.
func mediaStatusFromSysfs(devPath string) mediaState {
	name := extractDeviceName(devPath)

	if data, err := os.ReadFile("/sys/block/" + name + "/media"); err == nil {
		if strings.TrimSpace(string(data)) == "cdrom" {
			return mediaPresent
		}
		return mediaAbsent
	}

	if data, err := os.ReadFile("/sys/class/block/" + name + "/size"); err == nil {
		if size := strings.TrimSpace(string(data)); size != "" && size != "0" {
			return mediaPresent
		}
		return mediaAbsent
	}

	return mediaUnknown
}
