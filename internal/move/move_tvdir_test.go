package move

import (
	"os"
	"path/filepath"
	"testing"
)

// tv_shows_dir (#122): a TV move (season > 0) must land under tvDir, leaving
// moviesDir untouched; a movie move stays under moviesDir.
func TestMoveTVUsesSeparateTVDir(t *testing.T) {
	tmp := t.TempDir()
	ripTV := filepath.Join(tmp, "rip-tv")
	ripMovie := filepath.Join(tmp, "rip-movie")
	moviesDir := filepath.Join(tmp, "movies")
	tvDir := filepath.Join(tmp, "tvshows")
	for _, d := range []string{ripTV, ripMovie} {
		if err := os.MkdirAll(d, 0755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(ripTV, "title_t00.mkv"), make([]byte, 1<<20), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripMovie, "title_t00.mkv"), make([]byte, 1<<20), 0o644); err != nil {
		t.Fatal(err)
	}

	res, err := Move(ripTV, moviesDir, tvDir, "KLOVN", "2010", 1, 1)
	if err != nil {
		t.Fatalf("Move TV = %v", err)
	}
	want := filepath.Join(tvDir, "KLOVN (2010)", "Season 1", "KLOVN - S01E01 - title_t00.mkv")
	if res.Episodes[0].Path != want {
		t.Errorf("TV episode landed at %q, want %q", res.Episodes[0].Path, want)
	}
	if _, err := os.Stat(want); err != nil {
		t.Errorf("missing episode %v", err)
	}
	if _, err := os.Stat(filepath.Join(moviesDir, "KLOVN (2010)")); !os.IsNotExist(err) {
		t.Errorf("TV show leaked into moviesDir: %v", err)
	}

	res, err = Move(ripMovie, moviesDir, tvDir, "The Mask", "1994", 0, 0)
	if err != nil {
		t.Fatalf("Move movie = %v", err)
	}
	wantMain := filepath.Join(moviesDir, "The Mask (1994)", "The Mask (1994).mkv")
	if res.MainFile != wantMain {
		t.Errorf("movie landed at %q, want %q", res.MainFile, wantMain)
	}
	if _, err := os.Stat(filepath.Join(tvDir, "The Mask (1994)")); !os.IsNotExist(err) {
		t.Errorf("movie leaked into tvDir: %v", err)
	}
}

// tv_shows_dir (#122): MoveTitleFile with season > 0 must land the file under
// tvDir; movies and extras stay under moviesDir.
func TestMoveTitleFileRoutesTVToTVDir(t *testing.T) {
	tmp := t.TempDir()
	ripDir := filepath.Join(tmp, "rip")
	moviesDir := filepath.Join(tmp, "movies")
	tvDir := filepath.Join(tmp, "tvshows")
	if err := os.MkdirAll(ripDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "title_t00.mkv"), make([]byte, 1<<20), 0o644); err != nil {
		t.Fatal(err)
	}

	res, err := MoveTitleFile(ripDir, moviesDir, tvDir, "KLOVN", 2010, "title_t00.mkv", true, 1, 1)
	if err != nil {
		t.Fatalf("MoveTitleFile TV = %v", err)
	}
	// MoveTitleFile keeps the source base name inside the episode pattern, so
	// "title_t00.mkv" becomes "… - title_t00.mkv.mkv" (existing behavior).
	want := filepath.Join(tvDir, "KLOVN (2010)", "Season 1", "KLOVN - S01E01 - title_t00.mkv.mkv")
	if res.Dest != want {
		t.Errorf("TV title landed at %q, want %q", res.Dest, want)
	}

	if err := os.WriteFile(filepath.Join(ripDir, "title_t01.mkv"), make([]byte, 1<<20), 0o644); err != nil {
		t.Fatal(err)
	}
	res, err = MoveTitleFile(ripDir, moviesDir, tvDir, "The Mask", 1994, "title_t01.mkv", true, 0, 0)
	if err != nil {
		t.Fatalf("MoveTitleFile movie = %v", err)
	}
	wantMovie := filepath.Join(moviesDir, "The Mask (1994)", "The Mask (1994).mkv")
	if res.Dest != wantMovie {
		t.Errorf("movie title landed at %q, want %q", res.Dest, wantMovie)
	}
}
