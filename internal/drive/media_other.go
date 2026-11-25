//go:build !linux

package drive

// mediaStatus falls back to sysfs on non-Linux platforms.
func mediaStatus(devPath string) mediaState {
	return mediaStatusFromSysfs(devPath)
}
