package api

import (
	"bufio"
	"io/fs"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"riply/internal/logger"
)

// candidatePaths returns suggested directories for the settings path
// dropdowns: device-backed mount points plus well-known locations
// ($HOME, $HOME/Movies, /tmp). Sorted and deduplicated.
func candidatePaths() []string {
	seen := map[string]bool{}
	var out []string
	add := func(p string) {
		if p == "" || seen[p] {
			return
		}
		seen[p] = true
		out = append(out, p)
	}

	// Real mount points from /proc/mounts: keep device-backed filesystems
	// (ext4, btrfs, zfs, ...), skip virtual ones (proc, sysfs, tmpfs, ...).
	if f, err := os.Open("/proc/mounts"); err == nil {
		defer f.Close()
		sc := bufio.NewScanner(f)
		for sc.Scan() {
			fields := strings.Fields(sc.Text())
			if len(fields) < 3 {
				continue
			}
			dev, mnt, fstype := fields[0], fields[1], fields[2]
			if mnt == "/" || isVirtualFS(fstype) || dev == "none" {
				continue
			}
			if !strings.HasPrefix(mnt, "/") {
				continue
			}
			mnt = strings.ReplaceAll(mnt, "\\040", " ")
			if fi, err := os.Stat(mnt); err == nil && fi.IsDir() {
				add(mnt)
			}
		}
	}
	// Root is always a valid destination.
	add("/")

	// Well-known locations.
	if home, err := os.UserHomeDir(); err == nil {
		add(home)
		add(filepath.Join(home, "Movies"))
		add(filepath.Join(home, "Videos"))
	}
	add("/tmp")

	sort.Strings(out)
	return out
}

// isVirtualFS reports whether a filesystem type from /proc/mounts is a
// virtual/kernel mount rather than real storage.
func isVirtualFS(fstype string) bool {
	switch fstype {
	case "proc", "sysfs", "tmpfs", "devtmpfs", "devpts", "squashfs", "cgroup",
		"cgroup2", "securityfs", "pstore", "debugfs", "tracefs", "bpf", "fusectl",
		"hugetlbfs", "mqueue", "ramfs", "configfs", "efivarfs", "binder", "overlay",
		"nsfs", "rpc_pipefs", "autofs", "binfmt_misc", "fuse.snapfuse", "fuse.gvfs-fuse-daemon":
		return true
	}
	return strings.HasPrefix(fstype, "fuse") && !strings.Contains(fstype, "zfs")
}

// handlePaths serves GET /api/paths: candidate directories for the settings
// path dropdowns.
func (s *Server) handlePaths(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, map[string]any{"paths": candidatePaths()})
}

// handlePathsBrowse serves GET /api/paths/browse?dir=<path>: the real
// subdirectories of dir for the settings directory browser. Hidden
// (dot-prefixed) directories are skipped; symlinks to directories resolve.
// Any filesystem path is listable — Riply is a localhost, single-user service.
// Missing, non-directory, and unreadable paths all return 404 with no detail,
// so the endpoint cannot be probed for what exists.
func (s *Server) handlePathsBrowse(w http.ResponseWriter, r *http.Request) {
	dir := r.URL.Query().Get("dir")
	if dir == "" {
		if home, err := os.UserHomeDir(); err == nil {
			dir = home
		} else {
			dir = "/"
		}
	}
	dir = filepath.Clean(dir)
	if !filepath.IsAbs(dir) {
		dir = "/" + dir
	}

	fi, err := os.Stat(dir)
	if err != nil || !fi.IsDir() {
		w.WriteHeader(http.StatusNotFound)
		writeJSON(w, map[string]any{"error": "not found"})
		return
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		logger.Debug("browse failed", "dir", dir, "error", err)
		w.WriteHeader(http.StatusNotFound)
		writeJSON(w, map[string]any{"error": "not found"})
		return
	}

	type dirEntry struct {
		Name string `json:"name"`
		Path string `json:"path"`
	}
	var dirs []dirEntry
	for _, e := range entries {
		name := e.Name()
		if strings.HasPrefix(name, ".") {
			continue
		}
		if !e.IsDir() {
			if e.Type()&fs.ModeSymlink == 0 {
				continue
			}
			if fi, err := os.Stat(filepath.Join(dir, name)); err != nil || !fi.IsDir() {
				continue
			}
		}
		dirs = append(dirs, dirEntry{Name: name, Path: filepath.Join(dir, name)})
	}
	sort.Slice(dirs, func(i, j int) bool {
		return strings.ToLower(dirs[i].Name) < strings.ToLower(dirs[j].Name)
	})

	parent := ""
	if dir != "/" {
		parent = filepath.Clean(filepath.Dir(dir))
	}
	logger.Debug("browse", "dir", dir, "found", len(dirs))
	writeJSON(w, map[string]any{"dir": dir, "parent": parent, "dirs": dirs})
}
