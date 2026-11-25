package move

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestPreviewTitlesSequentialEpisodes(t *testing.T) {
	tmp := t.TempDir()
	ripDir := filepath.Join(tmp, "rip")
	moviesDir := filepath.Join(tmp, "movies")
	if err := os.MkdirAll(ripDir, 0755); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"A1_t00.mkv", "A1_t01.mkv"} {
		if err := os.WriteFile(filepath.Join(ripDir, name), make([]byte, 1024), 0644); err != nil {
			t.Fatal(err)
		}
	}

	results, err := PreviewTitles(ripDir, moviesDir, "Series", "2020", "tv", 2, 5)
	if err != nil {
		t.Fatal(err)
	}
	if len(results) != 2 {
		t.Fatalf("results = %d, want 2", len(results))
	}

	wantEps := []int{5, 6}
	for i, r := range results {
		if r.TvEpisode != wantEps[i] {
			t.Errorf("result %d TvEpisode = %d, want %d", i, r.TvEpisode, wantEps[i])
		}
		wantSuffix := "S02E" + map[int]string{5: "05", 6: "06"}[wantEps[i]]
		if !strings.Contains(r.ProposedDestination, wantSuffix) {
			t.Errorf("result %d path = %q, want segment %s", i, r.ProposedDestination, wantSuffix)
		}
		if !strings.Contains(r.ProposedDestination, filepath.Join("Series (2020)", "Season 2")) {
			t.Errorf("result %d path = %q, want Season 2 layout", i, r.ProposedDestination)
		}
	}
}

func TestPreviewTitlesMovieUnchanged(t *testing.T) {
	tmp := t.TempDir()
	ripDir := filepath.Join(tmp, "rip")
	moviesDir := filepath.Join(tmp, "movies")
	if err := os.MkdirAll(ripDir, 0755); err != nil {
		t.Fatal(err)
	}
	// A big main file and a smaller extra.
	if err := os.WriteFile(filepath.Join(ripDir, "main.mkv"), make([]byte, 4096), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ripDir, "extra.mkv"), make([]byte, 1024), 0644); err != nil {
		t.Fatal(err)
	}

	results, err := PreviewTitles(ripDir, moviesDir, "Movie", "2020", "movie", 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(results) != 2 {
		t.Fatalf("results = %d, want 2", len(results))
	}
	for _, r := range results {
		if r.TvSeason != 0 || r.TvEpisode != 0 {
			t.Errorf("movie result carries TV fields: %+v", r)
		}
	}
}
