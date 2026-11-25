package move

// getFreeSpace is the seam used by safeMove and tests; platform
// implementations live in fspace_linux.go / fspace_other.go.
var getFreeSpace = realGetFreeSpace

// FreeSpace reports the bytes available to the caller on the filesystem
// containing path. An error means the platform could not answer — callers
// treat that as "unknown", never as "no space".
func FreeSpace(path string) (uint64, error) {
	return getFreeSpace(path)
}
