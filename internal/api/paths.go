package api

import (
	"bufio"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"
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
