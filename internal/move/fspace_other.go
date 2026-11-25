//go:build !linux

package move

import "errors"

// realGetFreeSpace is unsupported outside Linux; callers treat the error as
// "unknown" and proceed.
func realGetFreeSpace(string) (uint64, error) {
	return 0, errors.New("free space query unsupported on this platform")
}
