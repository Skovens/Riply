//go:build linux

package drive

import (
	"bytes"
	"errors"
	"os"
	"syscall"
	"testing"
)

// L9: when the device cannot be opened (e.g. EACCES), the sysfs fallback
// reads attributes that were never refreshed by an open — detection can stay
// wrong for that drive. That must produce one Info log per drive path (the
// 2 s poll loop would otherwise silently repeat it forever).
func TestMediaStatusLogsStaleFallbackOncePerDrive(t *testing.T) {
	tmp, err := os.CreateTemp(t.TempDir(), "stderr")
	if err != nil {
		t.Fatal(err)
	}
	defer tmp.Close()

	// The logger writes through the fd behind os.Stderr, so redirect it for
	// the whole test and read the capture file after each phase.
	savedFD, err := syscall.Dup(syscall.Stderr)
	if err != nil {
		t.Fatal(err)
	}
	tmpFD := int(tmp.Fd())
	if err := syscall.Dup2(tmpFD, syscall.Stderr); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { syscall.Dup2(savedFD, syscall.Stderr) })

	const devPath = "/dev/riply-fake-sr99"
	origOpen := openDrivePath
	openDrivePath = func(string, int, uint32) (int, error) { return -1, errors.New("permission denied") }
	t.Cleanup(func() { openDrivePath = origOpen })

	mediaStatus(devPath) // first probe: warns
	mediaStatus(devPath) // second probe: must not warn again
	tmp.Sync()
	data, err := os.ReadFile(tmp.Name())
	if err != nil {
		t.Fatal(err)
	}
	if n := bytes.Count(data, []byte("falling back to sysfs; values may be stale")); n != 1 {
		t.Errorf("stale-fallback Info emitted %d times, want exactly 1\noutput:\n%s", n, data)
	}
	if !bytes.Contains(data, []byte(devPath)) {
		t.Errorf("warning does not include drive path %q:\n%s", devPath, data)
	}

	// A different drive path gets its own warning.
	mediaStatus("/dev/riply-fake-sr98")
	tmp.Sync()
	data2, err := os.ReadFile(tmp.Name())
	if err != nil {
		t.Fatal(err)
	}
	if n := bytes.Count(data2, []byte("falling back to sysfs; values may be stale")); n != 2 {
		t.Errorf("second drive did not get its own warning (count %d, want 2)", n)
	}
}
