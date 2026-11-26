package move

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// H2: an extra that fails to move must never be deleted — the rip dir is
// kept and the failure is reported instead of a silent Success:true.
// Failure is injected via the moveFile seam for one named destination.
func TestMoveKeepsRipDirWhenExtraFails(t *testing.T) {
	tmp := t.TempDir()
	ripDir := filepath.Join(tmp, "rip")
	moviesDir := filepath.Join(tmp, "movies")
	if err := os.MkdirAll(ripDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "main.mkv"), make([]byte, 10<<20), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "extra.mkv"), make([]byte, 1024), 0644); err != nil {
		t.Fatal(err)
	}

	orig := moveFile
	t.Cleanup(func() { moveFile = orig })
	moveFile = func(src, dst string) error {
		if filepath.Base(dst) == "extra.mkv" {
			return errors.New("injected I/O error")
		}
		return safeMove(src, dst)
	}

	result, err := Move(ripDir, moviesDir, moviesDir, "Title", "2020", 0, 0)
	if err == nil {
		t.Fatal("Move reported success despite failed extra")
	}
	if result == nil {
		t.Fatal("Move returned nil result with error")
	}
	if result.Success {
		t.Error("Success must be false on failed extra")
	}
	if result.FailedExtras != 1 {
		t.Errorf("FailedExtras = %d, want 1", result.FailedExtras)
	}
	if !strings.Contains(err.Error(), "1 extra(s)") {
		t.Errorf("error must name the failed count: %v", err)
	}

	// The failed extra must survive in the kept rip dir.
	if _, err := os.Stat(filepath.Join(ripDir, "extra.mkv")); err != nil {
		t.Errorf("failed extra deleted from rip dir: %v", err)
	}
	if _, err := os.Stat(ripDir); err != nil {
		t.Errorf("rip dir must be kept on partial failure: %v", err)
	}
	// Main file must have moved normally.
	if _, err := os.Stat(filepath.Join(moviesDir, "Title (2020)", "Title (2020).mkv")); err != nil {
		t.Errorf("main file not moved: %v", err)
	}
}

// Happy path unchanged: rip dir is cleaned up when everything moves.
func TestMoveRemovesRipDirWhenAllSucceed(t *testing.T) {
	tmp := t.TempDir()
	ripDir := filepath.Join(tmp, "rip")
	moviesDir := filepath.Join(tmp, "movies")
	if err := os.MkdirAll(ripDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "main.mkv"), make([]byte, 10<<20), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "extra.mkv"), make([]byte, 1024), 0644); err != nil {
		t.Fatal(err)
	}

	result, err := Move(ripDir, moviesDir, moviesDir, "Title", "2020", 0, 0)
	if err != nil {
		t.Fatalf("Move failed: %v", err)
	}
	if !result.Success || result.FailedExtras != 0 {
		t.Errorf("result = %+v, want full success", result)
	}
	if _, err := os.Stat(ripDir); !os.IsNotExist(err) {
		t.Errorf("rip dir should be removed on success: %v", err)
	}
	if _, err := os.Stat(filepath.Join(moviesDir, "Title (2020)", "Extras", "extra.mkv")); err != nil {
		t.Errorf("extra not moved: %v", err)
	}
}
