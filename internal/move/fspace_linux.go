//go:build linux

package move

import "syscall"

// realGetFreeSpace returns bytes available to unprivileged writers
// (Bavail) on the filesystem containing path.
func realGetFreeSpace(path string) (uint64, error) {
	var st syscall.Statfs_t
	if err := syscall.Statfs(path, &st); err != nil {
		return 0, err
	}
	return uint64(st.Bavail) * uint64(st.Bsize), nil
}
