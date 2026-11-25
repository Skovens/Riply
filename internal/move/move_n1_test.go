package move

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// N1: a TV move (season > 0) must land every MKV in
// {Series} ({Year})/Season {N}/{Series} - S{NN}E{NN} - {Title}.mkv
// instead of the movie layout {Series} ({Year})/{Series}.mkv.
func TestMoveTVSeasonLayout(t *testing.T) {
	tmp := t.TempDir()
	ripDir := filepath.Join(tmp, "rip")
	moviesDir := filepath.Join(tmp, "movies")
	if err := os.MkdirAll(ripDir, 0755); err != nil {
		t.Fatal(err)
	}
	// biggest file first in episode order
	if err := os.WriteFile(filepath.Join(ripDir, "title_t01.mkv"), make([]byte, 512), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "title_t00.mkv"), make([]byte, 1<<20), 0o644); err != nil {
		t.Fatal(err)
	}

	result, err := Move(ripDir, moviesDir, "The Test", "2015", 2, 3)
	if err != nil {
		t.Fatalf("Move TV = %v", err)
	}
	if !result.Success {
		t.Fatalf("result = %+v, want success", result)
	}
	if len(result.Episodes) != 2 {
		t.Fatalf("Episodes = %d, want 2", len(result.Episodes))
	}

	// Biggest file (t00) must be the first episode: S02E03.
	want1 := filepath.Join(moviesDir, "The Test (2015)", "Season 2", "The Test - S02E03 - title_t00.mkv")
	want2 := filepath.Join(moviesDir, "The Test (2015)", "Season 2", "The Test - S02E04 - title_t01.mkv")
	for _, want := range []string{want1, want2} {
		if _, err := os.Stat(want); err != nil {
			t.Errorf("missing episode %v: %v", want, err)
		}
	}
	if result.Episodes[0].Path != want1 || result.Episodes[0].Episode != 3 {
		t.Errorf("first landed = %+v, want %s ep3", result.Episodes[0], want1)
	}
	// Movie layout must NOT be produced.
	if _, err := os.Stat(filepath.Join(moviesDir, "The Test (2015)", "The Test.mkv")); err == nil {
		t.Error("movie-layout main file produced for a TV move")
	}
	// Rip dir cleaned up after full success.
	if _, err := os.Stat(ripDir); !os.IsNotExist(err) {
		t.Errorf("rip dir should be removed on success: %v", err)
	}
}

// N1: re-accepting an overlapping episode range must not overwrite existing
// files; uniqueDest appends (Copy N) to the collided episode.
func TestMoveTVCollisionKeepsBoth(t *testing.T) {
	tmp := t.TempDir()
	moviesDir := filepath.Join(tmp, "movies")

	rip1 := filepath.Join(tmp, "rip1")
	if err := os.MkdirAll(rip1, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rip1, "title_t00.mkv"), []byte("first"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Move(rip1, moviesDir, "Show", "2020", 1, 1); err != nil {
		t.Fatal(err)
	}

	rip2 := filepath.Join(tmp, "rip2")
	if err := os.MkdirAll(rip2, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rip2, "title_t00.mkv"), []byte("second"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Move(rip2, moviesDir, "Show", "2020", 1, 1); err != nil {
		t.Fatal(err)
	}

	seasonDir := filepath.Join(moviesDir, "Show (2020)", "Season 1")
	orig := filepath.Join(seasonDir, "Show - S01E01 - title_t00.mkv")
	data, err := os.ReadFile(orig)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "first" {
		t.Errorf("original episode overwritten: %q", data)
	}
	matches, _ := filepath.Glob(filepath.Join(seasonDir, "Show - S01E01 - title_t00 (Copy *.mkv"))
	if len(matches) != 1 {
		t.Errorf("expected exactly one (Copy N) collision file, got %q", matches)
	}
	if len(matches) == 1 {
		data, err = os.ReadFile(matches[0])
		if err != nil || string(data) != "second" {
			t.Errorf("collision copy content = %q err %v, want second", data, err)
		}
	}
}

// moveTV keeps the rip dir and reports partial failure (mirrors H2 for movies).
func TestMoveTVPartialFailureKeepsRipDir(t *testing.T) {
	tmp := t.TempDir()
	ripDir := filepath.Join(tmp, "rip")
	moviesDir := filepath.Join(tmp, "movies")
	if err := os.MkdirAll(ripDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "good.mkv"), make([]byte, 2<<20), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "bad.mkv"), make([]byte, 1024), 0o644); err != nil {
		t.Fatal(err)
	}

	orig := moveFile
	t.Cleanup(func() { moveFile = orig })
	moveFile = func(src, dst string) error {
		if filepath.Base(src) == "bad.mkv" {
			return os.ErrPermission
		}
		return safeMove(src, dst)
	}

	result, err := Move(ripDir, moviesDir, "Show", "2020", 1, 1)
	if err == nil {
		t.Fatal("expected error for failed episode move")
	}
	if result == nil || result.Success {
		t.Fatalf("result = %+v, want non-nil with Success=false", result)
	}
	if result.FailedExtras != 1 {
		t.Errorf("FailedExtras = %d, want 1", result.FailedExtras)
	}
	// The failed episode must survive in the kept rip dir...
	if _, err := os.Stat(filepath.Join(ripDir, "bad.mkv")); err != nil {
		t.Errorf("failed episode deleted from rip dir: %v", err)
	}
	// ...while the successful one landed in the season dir.
	if _, err := os.Stat(filepath.Join(moviesDir, "Show (2020)", "Season 1", "Show - S01E01 - good.mkv")); err != nil {
		t.Errorf("successful episode not moved: %v", err)
	}
}

// A TV move into a directory whose show folder already exists merges into
// the same folder rather than producing "(Copy N)".
func TestMoveTVMergesIntoExistingShowFolder(t *testing.T) {
	tmp := t.TempDir()
	moviesDir := filepath.Join(tmp, "movies")

	rip1 := filepath.Join(tmp, "rip1")
	if err := os.MkdirAll(rip1, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rip1, "a.mkv"), []byte("a"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Move(rip1, moviesDir, "Show", "2020", 1, 1); err != nil {
		t.Fatal(err)
	}

	rip2 := filepath.Join(tmp, "rip2")
	if err := os.MkdirAll(rip2, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rip2, "b.mkv"), []byte("b"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Move(rip2, moviesDir, "Show", "2020", 2, 1); err != nil {
		t.Fatal(err)
	}

	entries, err := os.ReadDir(moviesDir)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if strings.Contains(e.Name(), "(Copy") {
			t.Errorf("TV move created duplicate show folder %q; must merge", e.Name())
		}
	}
	for _, want := range []string{
		filepath.Join("Show (2020)", "Season 1", "Show - S01E01 - a.mkv"),
		filepath.Join("Show (2020)", "Season 2", "Show - S02E01 - b.mkv"),
	} {
		if _, err := os.Stat(filepath.Join(moviesDir, want)); err != nil {
			t.Errorf("missing %s: %v", want, err)
		}
	}
}
