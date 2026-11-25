package move

import (
	"errors"
	"io/fs"
	"path/filepath"
)

// diskUsage is the seam used by the fill-limit checks and tests; platform
// implementations live in fspace_linux.go / fspace_other.go.
var diskUsage = realDiskUsage

// DiskUsage reports total and free bytes on the filesystem containing path.
func DiskUsage(path string) (total, free uint64, err error) {
	return diskUsage(path)
}

// DirSize walks root and sums regular file sizes. Symlinks are not followed.
func DirSize(root string) (uint64, error) {
	var sum uint64
	err := filepath.WalkDir(root, func(_ string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !d.Type().IsRegular() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		sum += uint64(info.Size())
		return nil
	})
	return sum, err
}

// OverLimit reports whether adding need bytes to used bytes would push the
// filesystem's used percentage above limit (0-100). limit 0 disables the
// check.
func OverLimit(used, need, total uint64, limit int) bool {
	if limit <= 0 || total == 0 {
		return false
	}
	usedPct := (float64(used) + float64(need)) / float64(total) * 100
	return usedPct > float64(limit)
}

var errUnsupportedUsage = errors.New("disk usage query unsupported on this platform")
