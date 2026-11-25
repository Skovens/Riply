//go:build linux

package drive

import (
	"syscall"

	"riply/internal/logger"
)

// CDROM ioctl constants from <linux/cdrom.h>.
const (
	cdromDriveStatus = 0x5326 // CDROM_DRIVE_STATUS
	cdsNoInfo        = 0
	cdsNoDisc        = 1
	cdsTrayOpen      = 2
	cdsDriveNotReady = 3
	cdsDiscOK        = 4
)

// openDrivePath is the seam mediaStatus uses to open the block device; tests
// force EACCES/ENOENT failures to exercise the sysfs-fallback warning.
var openDrivePath = syscall.Open

// mediaStatus probes the drive for disc presence.
//
// sysfs (/sys/block/srX/media, /sys/class/block/srX/size) is only refreshed when
// the block device is opened, and the kernel does not poll optical drives for
// media changes (events_poll_msecs = -1). Opening the device and issuing
// CDROM_DRIVE_STATUS forces a fresh media check, so a hardware eject/insert is
// seen on the next probe.
func mediaStatus(devPath string) mediaState {
	fd, err := openDrivePath(devPath, syscall.O_RDONLY|syscall.O_NONBLOCK|syscall.O_CLOEXEC, 0)
	if err != nil {
		if err == syscall.ENOMEDIUM {
			return mediaAbsent
		}
		logger.Debug("Media probe open failed", "path", devPath, "error", err)
		// The device could not be opened, so its /sys attributes were never
		// refreshed by the open: whatever the fallback reports may be stale
		// (L9). One Info per drive — the poll loop hits this every 2 s.
		logSysfsFallbackStale(devPath, "open: "+err.Error())
		return mediaStatusFromSysfs(devPath)
	}
	defer syscall.Close(fd)

	r, _, errno := syscall.Syscall(syscall.SYS_IOCTL, uintptr(fd), cdromDriveStatus, 0)
	if errno != 0 {
		logger.Debug("Media probe ioctl failed", "path", devPath, "error", errno)
		return mediaStatusFromSysfs(devPath)
	}
	logger.Debug("Media probe", "path", devPath, "status", int(r))

	switch int(r) {
	case cdsDiscOK:
		return mediaPresent
	case cdsNoDisc, cdsTrayOpen:
		return mediaAbsent
	case cdsNoInfo:
		return mediaStatusFromSysfs(devPath)
	default: // CDS_DRIVE_NOT_READY (transient spin-up) or unexpected
		return mediaUnknown
	}
}
