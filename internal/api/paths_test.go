package api

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"slices"
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

func TestHandlePathsBrowse(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "b B"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "a"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, ".hidden"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "plainfile"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join(root, "a"), filepath.Join(root, "a-link")); err != nil {
		t.Fatal(err)
	}

	decode := func(rec *httptest.ResponseRecorder) struct {
		Dir    string `json:"dir"`
		Parent string `json:"parent"`
		Dirs   []struct {
			Name string `json:"name"`
			Path string `json:"path"`
		} `json:"dirs"`
	} {
		var out struct {
			Dir    string `json:"dir"`
			Parent string `json:"parent"`
			Dirs   []struct {
				Name string `json:"name"`
				Path string `json:"path"`
			} `json:"dirs"`
		}
		if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
			t.Fatalf("decode response: %v (%s)", err, rec.Body.String())
		}
		return out
	}

	t.Run("lists real dirs, sorted, no dot-dirs, no files, symlinked dirs included", func(t *testing.T) {
		rec := doJSON(t, s, http.MethodGet, "/api/paths/browse?dir="+url.QueryEscape(root), nil)
		if rec.Code != http.StatusOK {
			t.Fatalf("browse %q = %d %s", root, rec.Code, rec.Body.String())
		}
		out := decode(rec)
		if out.Dir != filepath.Clean(root) {
			t.Errorf("dir = %q, want %q", out.Dir, filepath.Clean(root))
		}
		got := make([]string, len(out.Dirs))
		for i, d := range out.Dirs {
			got[i] = d.Path
		}
		want := []string{
			filepath.Join(root, "a"),
			filepath.Join(root, "a-link"),
			filepath.Join(root, "b B"),
		}
		if !slices.Equal(got, want) {
			t.Errorf("dirs = %v, want %v", got, want)
		}
	})

	t.Run("cleans dotdot and trailing slash", func(t *testing.T) {
		rec := doJSON(t, s, http.MethodGet, "/api/paths/browse?dir="+url.QueryEscape(root+"/../"+filepath.Base(root)+"/"), nil)
		if rec.Code != http.StatusOK {
			t.Fatalf("browse with .. = %d %s", rec.Code, rec.Body.String())
		}
		if out := decode(rec); out.Dir != filepath.Clean(root) {
			t.Errorf("dir = %q, want %q", out.Dir, filepath.Clean(root))
		}
	})

	t.Run("missing path is 404", func(t *testing.T) {
		rec := doJSON(t, s, http.MethodGet, "/api/paths/browse?dir="+url.QueryEscape(filepath.Join(root, "nope")), nil)
		if rec.Code != http.StatusNotFound {
			t.Errorf("missing dir = %d %s, want 404", rec.Code, rec.Body.String())
		}
	})

	t.Run("plain file is 404", func(t *testing.T) {
		rec := doJSON(t, s, http.MethodGet, "/api/paths/browse?dir="+url.QueryEscape(filepath.Join(root, "plainfile")), nil)
		if rec.Code != http.StatusNotFound {
			t.Errorf("file dir = %d %s, want 404", rec.Code, rec.Body.String())
		}
	})

	if os.Getuid() != 0 {
		t.Run("unreadable dir is 404", func(t *testing.T) {
			noread := filepath.Join(root, "noread")
			if err := os.Mkdir(noread, 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.Chmod(noread, 0o000); err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = os.Chmod(noread, 0o755) })
			rec := doJSON(t, s, http.MethodGet, "/api/paths/browse?dir="+url.QueryEscape(noread), nil)
			if rec.Code != http.StatusNotFound {
				t.Errorf("unreadable dir = %d %s, want 404", rec.Code, rec.Body.String())
			}
		})
	}

	t.Run("root has empty parent", func(t *testing.T) {
		rec := doJSON(t, s, http.MethodGet, "/api/paths/browse?dir=%2F", nil)
		if rec.Code != http.StatusOK {
			t.Fatalf("browse / = %d %s", rec.Code, rec.Body.String())
		}
		out := decode(rec)
		if out.Dir != "/" || out.Parent != "" {
			t.Errorf("root browse = dir %q parent %q, want / and empty", out.Dir, out.Parent)
		}
	})

	t.Run("empty dir defaults to home", func(t *testing.T) {
		rec := doJSON(t, s, http.MethodGet, "/api/paths/browse", nil)
		if rec.Code != http.StatusOK {
			t.Fatalf("browse default = %d %s", rec.Code, rec.Body.String())
		}
		out := decode(rec)
		home, err := os.UserHomeDir()
		if err != nil || home == "" {
			t.Skip("no home dir")
		}
		if out.Dir != home {
			t.Errorf("default dir = %q, want %q", out.Dir, home)
		}
	})
}
