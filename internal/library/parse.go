package library

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"

	"riply/internal/logger"
	"riply/internal/naming"
)

// videoExtensions are the file extensions treated as owned media.
var videoExtensions = map[string]bool{
	".mkv": true, ".mp4": true, ".m4v": true, ".avi": true,
	".ts": true, ".mov": true, ".wmv": true, ".mpg": true, ".mpeg": true,
}

var (
	yearNameRe  = regexp.MustCompile(`^(.*?)\s*\((\d{4})\)\s*$`)
	seasonDirRe = regexp.MustCompile(`(?i)^Season\s*(\d{1,2})$`)
)

// ParseYearName splits "{Title} ({Year})"; without a trailing (YYYY) it returns s and 0.
func ParseYearName(s string) (title string, year int) {
	if m := yearNameRe.FindStringSubmatch(s); m != nil {
		y := 0
		fmt.Sscanf(m[2], "%d", &y)
		t := strings.TrimSpace(m[1])
		if t == "" {
			return s, 0
		}
		return t, y
	}
	return s, 0
}

// ParsePath maps a slash-separated library-relative path to an Entry.
// It returns false for non-video files, hidden components, and anything under Extras/.
func ParsePath(rel string) (Entry, bool) {
	if rel == "" {
		return Entry{}, false
	}
	var parts []string
	for _, p := range strings.Split(rel, "/") {
		p = strings.TrimSpace(p)
		if p == "" {
			continue
		}
		parts = append(parts, p)
	}
	if len(parts) == 0 {
		return Entry{}, false
	}
	for _, p := range parts {
		if strings.HasPrefix(p, ".") {
			return Entry{}, false
		}
		if strings.EqualFold(p, "Extras") {
			return Entry{}, false
		}
	}
	name := parts[len(parts)-1]
	ext := strings.ToLower(filepath.Ext(name))
	if !videoExtensions[ext] {
		return Entry{}, false
	}

	seasonIdx := -1
	for i := len(parts) - 1; i >= 0; i-- {
		if seasonDirRe.MatchString(parts[i]) {
			seasonIdx = i
			break
		}
	}

	if seasonIdx >= 1 {
		title, year := ParseYearName(parts[seasonIdx-1])
		if title == "" {
			title = parts[seasonIdx-1]
		}
		m := seasonDirRe.FindStringSubmatch(parts[seasonIdx])
		seasonNum := 0
		fmt.Sscanf(m[1], "%d", &seasonNum)
		_, episode := naming.ParseSeasonEpisode(parts[len(parts)-1])
		return Entry{
			Title:     title,
			Year:      year,
			MediaType: "tv",
			Season:    seasonNum,
			Episode:   episode,
			Source:    "scan",
		}, true
	}

	var title string
	year := 0
	if len(parts) >= 2 {
		title, year = ParseYearName(parts[len(parts)-2])
		if title == "" {
			title = parts[len(parts)-2]
		}
	} else {
		stem := strings.TrimSuffix(name, filepath.Ext(name))
		title, year = ParseYearName(stem)
		if title == "" {
			title = stem
		}
	}
	return Entry{
		Title:     title,
		Year:      year,
		MediaType: "movie",
		Source:    "scan",
	}, true
}

// Scan walks moviesDir and upserts every parseable media file into the index.
// Files that previously existed but are gone now are flagged missing; entries
// outside the scanned root are left untouched. The scan never deletes files.
func (s *Store) Scan(ctx context.Context, moviesDir string) (ScanResult, error) {
	root, err := filepath.Abs(moviesDir)
	if err != nil {
		return ScanResult{}, fmt.Errorf("library: resolve %s: %w", moviesDir, err)
	}
	info, err := os.Stat(root)
	if err != nil || !info.IsDir() {
		return ScanResult{}, fmt.Errorf("library: %s is not a directory", moviesDir)
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	found := make(map[string]Entry)
	err = filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			if path == root {
				return err
			}
			logger.Warn("Library scan walk error", "path", path, "error", err)
			if d != nil && d.IsDir() {
				return fs.SkipDir
			}
			return nil
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		name := d.Name()
		if d.IsDir() {
			if path != root && (name == ".git" || name == ".trash" || strings.HasPrefix(name, ".")) {
				return fs.SkipDir
			}
			return nil
		}
		if strings.HasPrefix(name, ".") {
			return nil
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return nil
		}
		e, ok := ParsePath(filepath.ToSlash(rel))
		if !ok {
			return nil
		}
		e.OutputPath = path
		found[path] = e
		return nil
	})
	if err != nil {
		return ScanResult{}, fmt.Errorf("library: scan %s: %w", root, err)
	}

	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return ScanResult{}, fmt.Errorf("library: begin scan: %w", err)
	}
	commitFailed := true
	defer func() {
		if commitFailed {
			_ = tx.Rollback()
		}
	}()

	existing := make(map[string]bool)
	rows, err := tx.QueryContext(ctx, `SELECT output_path FROM library`)
	if err != nil {
		return ScanResult{}, fmt.Errorf("library: read existing paths: %w", err)
	}
	for rows.Next() {
		var p string
		if err := rows.Scan(&p); err != nil {
			rows.Close()
			return ScanResult{}, fmt.Errorf("library: scan existing paths: %w", err)
		}
		existing[p] = true
	}
	if err := rows.Err(); err != nil {
		rows.Close()
		return ScanResult{}, fmt.Errorf("library: read existing paths: %w", err)
	}
	rows.Close()

	var res ScanResult
	paths := make([]string, 0, len(found))
	for p := range found {
		paths = append(paths, p)
	}
	sort.Strings(paths)
	for _, p := range paths {
		e := found[p]
		if _, err := tx.ExecContext(ctx, upsertEntrySQL,
			e.DiscName, e.DiscHash, e.Title, e.Year, e.MediaType, e.Season, e.Episode,
			e.TMDBID, e.OutputPath, e.Source, 0, e.MovedAt, e.CreatedAt); err != nil {
			return ScanResult{}, fmt.Errorf("library: upsert %q: %w", p, err)
		}
		if existing[p] {
			res.Updated++
		} else {
			res.Added++
		}
	}

	prefix := root + string(os.PathSeparator)
	type stale struct {
		id   int64
		path string
	}
	var toFlag []stale
	rows, err = tx.QueryContext(ctx, `SELECT id, output_path FROM library WHERE missing = 0`)
	if err != nil {
		return ScanResult{}, fmt.Errorf("library: read entries for missing check: %w", err)
	}
	for rows.Next() {
		var st stale
		if err := rows.Scan(&st.id, &st.path); err != nil {
			rows.Close()
			return ScanResult{}, fmt.Errorf("library: scan entries for missing check: %w", err)
		}
		if strings.HasPrefix(st.path, prefix) {
			if _, ok := found[st.path]; !ok {
				toFlag = append(toFlag, st)
			}
		}
	}
	if err := rows.Err(); err != nil {
		rows.Close()
		return ScanResult{}, fmt.Errorf("library: read entries for missing check: %w", err)
	}
	rows.Close()
	for _, st := range toFlag {
		if _, err := tx.ExecContext(ctx, `UPDATE library SET missing = 1 WHERE id = ?`, st.id); err != nil {
			return ScanResult{}, fmt.Errorf("library: flag %q missing: %w", st.path, err)
		}
		res.Missing++
	}

	res.ScannedAt = time.Now().UTC().Format(time.RFC3339)
	if _, err := tx.ExecContext(ctx,
		`INSERT INTO meta(key,value) VALUES('scanned_at',?) ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
		res.ScannedAt); err != nil {
		return ScanResult{}, fmt.Errorf("library: record scanned_at: %w", err)
	}
	if err := tx.Commit(); err != nil {
		return ScanResult{}, fmt.Errorf("library: commit scan: %w", err)
	}
	commitFailed = false

	logger.Info("Library scanned", "dir", root, "found", len(found),
		"added", res.Added, "updated", res.Updated, "missing", res.Missing)
	return res, nil
}
