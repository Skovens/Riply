package move

import (
	"os"
	"path/filepath"
	"testing"
)

// H3: re-accepting the identical title/year/episode must land in a distinct
// path, not silently overwrite the library file via os.Rename.
func TestMoveTitleFileCollisionCopySuffix(t *testing.T) {
	tmp := t.TempDir()
	moviesDir := filepath.Join(tmp, "movies")

	mkRip := func(name, content string) string {
		d := filepath.Join(tmp, name)
		if err := os.MkdirAll(d, 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(d, "title.mkv"), []byte(content), 0644); err != nil {
			t.Fatal(err)
		}
		return d
	}

	r1 := mkRip("rip1", "first")
	r2 := mkRip("rip2", "second")

	res1, err := MoveTitleFile(r1, moviesDir, moviesDir, "Inception", 2010, "title.mkv", true, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	res2, err := MoveTitleFile(r2, moviesDir, moviesDir, "Inception", 2010, "title.mkv", true, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	if res1.Dest == res2.Dest {
		t.Fatalf("both accepts landed on the same dest: %s", res1.Dest)
	}
	if res2.Dest != filepath.Join(moviesDir, "Inception (2010)", "Inception (2010) (Copy 1).mkv") {
		t.Errorf("second dest = %s, want '(Copy 1)' suffix", res2.Dest)
	}
	b1, _ := os.ReadFile(res1.Dest)
	b2, _ := os.ReadFile(res2.Dest)
	if string(b1) != "first" || string(b2) != "second" {
		t.Errorf("contents clobbered: %q / %q", b1, b2)
	}
}

// H3: two same-base extras from different rips must coexist.
func TestMoveTitleFileExtrasCoexist(t *testing.T) {
	tmp := t.TempDir()
	moviesDir := filepath.Join(tmp, "movies")

	mkRip := func(name, content string) string {
		d := filepath.Join(tmp, name)
		if err := os.MkdirAll(d, 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(d, "behindthescenes.mkv"), []byte(content), 0644); err != nil {
			t.Fatal(err)
		}
		return d
	}

	r1 := mkRip("rip1", "disc-a-extra")
	r2 := mkRip("rip2", "disc-b-extra")

	res1, err := MoveTitleFile(r1, moviesDir, moviesDir, "Inception", 2010, "behindthescenes.mkv", false, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	res2, err := MoveTitleFile(r2, moviesDir, moviesDir, "Inception", 2010, "behindthescenes.mkv", false, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	if res1.Dest == res2.Dest {
		t.Fatalf("both extras landed on the same dest: %s", res1.Dest)
	}
	b1, _ := os.ReadFile(res1.Dest)
	b2, _ := os.ReadFile(res2.Dest)
	if string(b1) != "disc-a-extra" || string(b2) != "disc-b-extra" {
		t.Errorf("extra contents clobbered: %q / %q", b1, b2)
	}
}

// H3: TV re-accept of identical series/season/episode lands distinctly.
func TestMoveTitleFileTVReAcceptDistinct(t *testing.T) {
	tmp := t.TempDir()
	moviesDir := filepath.Join(tmp, "movies")

	mkRip := func(name, content string) string {
		d := filepath.Join(tmp, name)
		if err := os.MkdirAll(d, 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(d, "ep.mkv"), []byte(content), 0644); err != nil {
			t.Fatal(err)
		}
		return d
	}

	r1 := mkRip("rip1", "s1e1")
	r2 := mkRip("rip2", "s1e1-take2")

	res1, err := MoveTitleFile(r1, moviesDir, moviesDir, "KLOVN", 0, "ep.mkv", false, 1, 1)
	if err != nil {
		t.Fatal(err)
	}
	res2, err := MoveTitleFile(r2, moviesDir, moviesDir, "KLOVN", 0, "ep.mkv", false, 1, 1)
	if err != nil {
		t.Fatal(err)
	}
	if res1.Dest == res2.Dest {
		t.Fatalf("both TV accepts landed on the same dest: %s", res1.Dest)
	}
}
