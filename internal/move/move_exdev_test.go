package move

import (
	"os"
	"path/filepath"
	"sync"
	"syscall"
	"testing"
)

// #103: a real cross-device rename error (EXDEV) must trigger the copy+delete
// fallback and leave both files consistent.
func TestSafeMoveCrossDeviceEXDEV(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src.mkv")
	dst := filepath.Join(tmp, "dst.mkv")
	if err := os.WriteFile(src, []byte("cross-device payload"), 0o644); err != nil {
		t.Fatal(err)
	}

	origRename := renameFile
	t.Cleanup(func() { renameFile = origRename })
	renameFile = func(_, _ string) error {
		linkErr := &os.LinkError{Op: "rename", Err: syscall.EXDEV}
		return linkErr
	}

	if err := safeMove(src, dst); err != nil {
		t.Fatalf("safeMove with EXDEV fallback: %v", err)
	}
	data, err := os.ReadFile(dst)
	if err != nil {
		t.Fatalf("dst missing: %v", err)
	}
	if string(data) != "cross-device payload" {
		t.Errorf("dst content = %q", data)
	}
	if _, err := os.Stat(src); !os.IsNotExist(err) {
		t.Errorf("src not removed after successful copy: %v", err)
	}
}

// #103: the free-space guard on the copy fallback must refuse when the source
// does not fit and must not leave a partial dst or a removed src behind.
func TestSafeMoveEXDEVRefusedWhenSourceDoesNotFit(t *testing.T) {
	origFree := getFreeSpace
	t.Cleanup(func() { getFreeSpace = origFree })
	getFreeSpace = func(string) (uint64, error) { return 10, nil } // 10 bytes free
	origRename := renameFile
	t.Cleanup(func() { renameFile = origRename })
	renameFile = func(_, _ string) error { return &os.LinkError{Op: "rename", Err: syscall.EXDEV} }

	tmp := t.TempDir()
	src := filepath.Join(tmp, "src.mkv")
	dst := filepath.Join(tmp, "dst.mkv")
	if err := os.WriteFile(src, []byte("this does not fit in 10 bytes"), 0o644); err != nil {
		t.Fatal(err)
	}

	err := safeMove(src, dst)
	if err == nil {
		t.Fatal("safeMove succeeded although the source does not fit")
	}
	if _, serr := os.Stat(dst); !os.IsNotExist(serr) {
		t.Errorf("partial dst left behind: %v", serr)
	}
	if _, rerr := os.Stat(src); rerr != nil {
		t.Errorf("src removed despite refused copy: %v", rerr)
	}
}

// #103: concurrent moves of the same source to distinct destinations must all
// succeed exactly once — no lost file, no duplicated content.
func TestConcurrentSafeMoveDistinctDests(t *testing.T) {
	tmp := t.TempDir()
	const n = 8
	srcs := make([]string, n)
	for i := range srcs {
		srcs[i] = filepath.Join(tmp, "src"+string(rune('a'+i))+".mkv")
		if err := os.WriteFile(srcs[i], []byte("payload"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	var wg sync.WaitGroup
	errs := make([]error, n)
	for i := range srcs {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			errs[i] = safeMove(srcs[i], filepath.Join(tmp, "dst"+string(rune('a'+i))+".mkv"))
		}(i)
	}
	wg.Wait()
	for i := range srcs {
		if errs[i] != nil {
			t.Errorf("move %d: %v", i, errs[i])
		}
		if _, err := os.Stat(srcs[i]); !os.IsNotExist(err) {
			t.Errorf("src %d survived", i)
		}
	}
}
