package api

import (
	"encoding/json"
	"net/http"
	"os"
	"strings"
	"testing"
)

func TestHandlePathsReturnsMountsAndWellKnownDirs(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	rec := doJSON(t, s, http.MethodGet, "/api/paths", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("GET /api/paths = %d %s", rec.Code, rec.Body.String())
	}

	var out struct {
		Paths []string `json:"paths"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode response: %v (%s)", err, rec.Body.String())
	}

	if len(out.Paths) == 0 {
		t.Fatal("paths is empty, want at least / and well-known dirs")
	}

	// Sorted, deduplicated.
	for i := 1; i < len(out.Paths); i++ {
		if out.Paths[i] < out.Paths[i-1] {
			t.Errorf("paths not sorted: %q before %q", out.Paths[i-1], out.Paths[i])
		}
		if out.Paths[i] == out.Paths[i-1] {
			t.Errorf("duplicate path %q", out.Paths[i])
		}
	}

	// Root is always offered.
	hasRoot := false
	for _, p := range out.Paths {
		if p == "/" {
			hasRoot = true
		}
	}
	if !hasRoot {
		t.Error("paths missing /")
	}

	// $HOME is offered.
	home, err := os.UserHomeDir()
	if err != nil {
		t.Skipf("no home dir: %v", err)
	}
	hasHome := false
	for _, p := range out.Paths {
		if p == home {
			hasHome = true
		}
	}
	if !hasHome {
		t.Errorf("paths missing $HOME (%s); got %v", home, out.Paths)
	}
}

func TestCandidatePathsSkipsVirtualFS(t *testing.T) {
	paths := candidatePaths()
	for _, p := range paths {
		for _, virtual := range []string{"/proc", "/sys", "/dev", "/run"} {
			if p == virtual || strings.HasPrefix(p, virtual+"/") {
				t.Errorf("virtual mount %q leaked into candidates: %v", p, paths)
			}
		}
	}
}
