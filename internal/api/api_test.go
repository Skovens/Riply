package api

import (
	"os"
	"path/filepath"
	"testing"

	"riply/internal/naming"
)

func TestSearchableName(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  bool
	}{
		{"empty", "", false},
		{"spaces", "  ", false},
		{"Unknown", "Unknown", false},
		{"unknown", "unknown", false},
		{"UNKNOWN", "UNKNOWN", false},
		{"MASK", "MASK", true},
		{"  MASK  ", "  MASK  ", true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := searchableName(tt.input)
			if got != tt.want {
				t.Errorf("searchableName(%q) = %v, want %v", tt.input, got, tt.want)
			}
		})
	}
}

func TestClassifyRip(t *testing.T) {
	// Movie: one big file
	dir1 := t.TempDir()
	os.WriteFile(filepath.Join(dir1, "main.mkv"), make([]byte, 10*1024*1024), 0644)
	mediaType, _ := classifyRip(dir1)
	if mediaType != "movie" {
		t.Errorf("classifyRip(one file) = %q, want %q", mediaType, "movie")
	}

	// Movie: two similar files
	dir2 := t.TempDir()
	os.WriteFile(filepath.Join(dir2, "main.mkv"), make([]byte, 10*1024*1024), 0644)
	os.WriteFile(filepath.Join(dir2, "extra.mkv"), make([]byte, 10*1024*1024), 0644)
	mediaType2, _ := classifyRip(dir2)
	if mediaType2 != "movie" {
		t.Errorf("classifyRip(two similar files) = %q, want %q", mediaType2, "movie")
	}

	// TV: three files, one big + two small
	dir3 := t.TempDir()
	os.WriteFile(filepath.Join(dir3, "main.mkv"), make([]byte, 10*1024*1024), 0644)
	os.WriteFile(filepath.Join(dir3, "ep1.mkv"), make([]byte, 2*1024*1024), 0644)
	os.WriteFile(filepath.Join(dir3, "ep2.mkv"), make([]byte, 2*1024*1024), 0644)
	mediaType3, _ := classifyRip(dir3)
	if mediaType3 != "tv" {
		t.Errorf("classifyRip(three files) = %q, want %q", mediaType3, "tv")
	}
}

func TestPopulateTitles(t *testing.T) {
	tmpDir := t.TempDir()
	moviesDir := filepath.Join(tmpDir, "movies")
	os.MkdirAll(moviesDir, 0755)

	ripDir := filepath.Join(tmpDir, "rip")
	os.MkdirAll(ripDir, 0755)

	// Create test files
	os.WriteFile(filepath.Join(ripDir, "A1_t00.mkv"), make([]byte, 10*1024*1024), 0644)
	os.WriteFile(filepath.Join(ripDir, "A1_t01.mkv"), make([]byte, 2*1024*1024), 0644)

	// Test 1: No match, movie type
	titles := populateTitles(ripDir, moviesDir, nil, "movie", 0, 0)
	if len(titles) != 2 {
		t.Errorf("populateTitles(no match) count = %d, want 2", len(titles))
	}
	if !titles[0].IsMainFile {
		t.Error("populateTitles(no tmdb) first title should be main")
	}

	// Test 2: With match, movie type
	match := &naming.Result{Title: "A", Year: 2020}
	titles2 := populateTitles(ripDir, moviesDir, match, "movie", 0, 0)
	if len(titles2) != 2 {
		t.Errorf("populateTitles(match) count = %d, want 2", len(titles2))
	}

	// Main file should have destination like /moviesDir/A (2020)/A (2020).mkv
	expectedMain := filepath.Join(moviesDir, "A (2020)", "A (2020).mkv")
	if titles2[0].ProposedDestination != expectedMain {
		t.Errorf("populateTitles(match) main destination = %q, want %q", titles2[0].ProposedDestination, expectedMain)
	}

	// Extra file should go to Extras/
	expectedExtra := filepath.Join(moviesDir, "A (2020)", "Extras", "A1_t01.mkv")
	if titles2[1].ProposedDestination != expectedExtra {
		t.Errorf("populateTitles(match) extra destination = %q, want %q", titles2[1].ProposedDestination, expectedExtra)
	}
}

func TestResolveMediaType(t *testing.T) {
	tests := []struct {
		name       string
		classified string
		match      *naming.Result
		want       string
	}{
		{"tv classification, movie match", "tv", &naming.Result{IsTV: false}, "movie"},
		{"movie classification, tv match", "movie", &naming.Result{IsTV: true}, "tv"},
		{"no match keeps tv heuristic", "tv", nil, "tv"},
		{"no match keeps movie heuristic", "movie", nil, "movie"},
		{"unknown without match stays unknown", "unknown", nil, "unknown"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := resolveMediaType(tt.classified, tt.match); got != tt.want {
				t.Errorf("resolveMediaType(%q, %+v) = %q, want %q", tt.classified, tt.match, got, tt.want)
			}
		})
	}
}
