package move

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// M10: if the cross-device copy fails midway, the partial destination must
// be removed (not linger to masquerade as a complete file), and src kept.
func TestSafeMoveRemovesPartialDstOnCopyFailure(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src.mkv")
	dst := filepath.Join(tmp, "dst.mkv")
	if err := os.WriteFile(src, []byte("0123456789"), 0o644); err != nil {
		t.Fatal(err)
	}

	origRename, origCopy := renameFile, copyData
	t.Cleanup(func() { renameFile, copyData = origRename, origCopy })

	renameFile = func(_, _ string) error { return errors.New("cross-device (simulated)") }
	copyData = func(dstW io.Writer, srcR io.Reader) (int64, error) {
		// Write a partial payload, then fail — like ENOSPC mid-copy.
		buf := make([]byte, 4)
		n, _ := srcR.Read(buf)
		_, _ = dstW.Write(buf[:n])
		return int64(n), errors.New("write failed (simulated ENOSPC)")
	}

	err := safeMove(src, dst)
	if err == nil {
		t.Fatal("safeMove reported success despite injected copy failure")
	}
	if _, err := os.Stat(dst); !os.IsNotExist(err) {
		t.Errorf("partial dst left behind: %v", err)
	}
	data, rerr := os.ReadFile(src)
	if rerr != nil {
		t.Fatalf("src removed on failed copy: %v", rerr)
	}
	if string(data) != "0123456789" {
		t.Errorf("src modified: %q", data)
	}
}

// The copy fallback itself must still land fully and remove src when the
// rename is unavailable.
func TestSafeMoveCopyFallbackSucceeds(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src.mkv")
	dst := filepath.Join(tmp, "dst.mkv")
	if err := os.WriteFile(src, []byte("payload"), 0o644); err != nil {
		t.Fatal(err)
	}

	origRename := renameFile
	t.Cleanup(func() { renameFile = origRename })
	renameFile = func(_, _ string) error { return errors.New("cross-device (simulated)") }

	if err := safeMove(src, dst); err != nil {
		t.Fatalf("safeMove copy fallback = %v", err)
	}
	data, err := os.ReadFile(dst)
	if err != nil || string(data) != "payload" {
		t.Errorf("dst = %q err %v, want payload", data, err)
	}
	if _, err := os.Stat(src); !os.IsNotExist(err) {
		t.Errorf("src still present after successful copy-move")
	}
}

// L5: SanitizeName must reject "."/".." (they used to survive the character
// filter and let an accepted title of ".." escape moviesDir) and Windows
// reserved device names, mapping them to safe distinct names.
func TestSanitizeNameRejectsDangerousElements(t *testing.T) {
	cases := []struct{ in, want string }{
		{"..", "Untitled"},
		{".", "Untitled"},
		{"...", "..."}, // three dots is a legal (if odd) name, not an escape
		{"CON", "Untitled-CON"},
		{"con", "Untitled-con"},
		{"NUL.txt", "Untitled-NUL.txt"},
		{"COM1.mkv", "Untitled-COM1.mkv"},
		{"aux", "Untitled-aux"},
		{"LPT9", "Untitled-LPT9"},
		// ordinary names untouched (parens are stripped by the char filter)
		{"Contact", "Contact"},
		{"Contacts", "Contacts"},
		{"Null Pointers", "Null Pointers"},
	}
	seen := map[string]string{}
	for _, c := range cases {
		got := SanitizeName(c.in)
		if got != c.want {
			t.Errorf("SanitizeName(%q) = %q, want %q", c.in, got, c.want)
		}
		// Must never return a path-escaping or reserved element itself.
		if got == "." || got == ".." || got == c.in && (strings.EqualFold(c.in, "con") || strings.EqualFold(c.in, "nul.txt")) {
			t.Errorf("SanitizeName(%q) = %q: dangerous element survived", c.in, got)
		}
		seen[got] = c.in
	}
	// Distinctness: each dangerous input gets a distinct safe output.
	if len(seen) < 9 {
		t.Errorf("expected >=9 distinct sanitized results, got %d", len(seen))
	}
}

// L7: when the copy fallback would not fit on the destination volume, it
// must refuse before writing anything — a partial write would leave a
// corrupted file (and, pre-M10, would then masquerade as complete).
func TestSafeMoveRefusesWhenNoSpace(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src.mkv")
	dst := filepath.Join(tmp, "dst.mkv")
	if err := os.WriteFile(src, make([]byte, 4<<20), 0o644); err != nil {
		t.Fatal(err)
	}

	origRename, origFree := renameFile, getFreeSpace
	t.Cleanup(func() { renameFile, getFreeSpace = origRename, origFree })
	renameFile = func(_, _ string) error { return errors.New("cross-device (simulated)") }
	getFreeSpace = func(string) (uint64, error) { return 1 << 20, nil } // 1 MiB < 4 MiB src

	err := safeMove(src, dst)
	if err == nil {
		t.Fatal("safeMove copied despite insufficient space")
	}
	if !strings.Contains(err.Error(), "insufficient space") {
		t.Errorf("error = %v, want insufficient space", err)
	}
	if _, err := os.Stat(dst); !os.IsNotExist(err) {
		t.Errorf("dst created despite refusal: %v", err)
	}
	if _, err := os.Stat(src); err != nil {
		t.Errorf("src touched after refusal: %v", err)
	}

	// With enough reported space the copy proceeds.
	getFreeSpace = func(string) (uint64, error) { return 1 << 30, nil }
	if err := safeMove(src, dst); err != nil {
		t.Fatalf("safeMove with ample space = %v", err)
	}
	if _, err := os.Stat(dst); err != nil {
		t.Errorf("dst missing after ok-space move: %v", err)
	}
}

// An unknown free-space answer (unsupported platform) must NOT block the
// copy fallback.
func TestSafeMoveProceedsWhenSpaceUnknown(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src.mkv")
	dst := filepath.Join(tmp, "dst.mkv")
	if err := os.WriteFile(src, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	origRename, origFree := renameFile, getFreeSpace
	t.Cleanup(func() { renameFile, getFreeSpace = origRename, origFree })
	renameFile = func(_, _ string) error { return errors.New("cross-device (simulated)") }
	getFreeSpace = func(string) (uint64, error) { return 0, errors.New("unsupported") }
	if err := safeMove(src, dst); err != nil {
		t.Fatalf("safeMove failed on unknown space: %v", err)
	}
}
