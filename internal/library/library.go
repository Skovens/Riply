// Package library persists Riply's history of owned media (accepted rips and
// scanned files) in a local SQLite database, and derives per-series next-up
// positions from it. It is an index only: resetting or losing the database
// never touches the media files themselves.
package library

import (
	"context"
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	_ "modernc.org/sqlite"

	"riply/internal/logger"
)

// Entry is one owned media file: a movie or a single TV episode.
type Entry struct {
	ID         int64  `json:"id"`
	DiscName   string `json:"disc_name,omitempty"`
	DiscHash   string `json:"disc_hash,omitempty"` // disc fingerprint (SHA-256 of TINFO metadata)
	Title      string `json:"title"`
	Year       int    `json:"year,omitempty"`
	MediaType  string `json:"media_type"` // "movie" | "tv"
	Season     int    `json:"season,omitempty"`
	Episode    int    `json:"episode,omitempty"`
	TMDBID     int    `json:"tmdb_id,omitempty"`
	OutputPath string `json:"output_path"`
	Source     string `json:"source"` // "rip" | "scan"
	Missing    bool   `json:"missing"`
	MovedAt    string `json:"moved_at,omitempty"`
	CreatedAt  string `json:"created_at"`
}

// SeriesSeason is one season of a series with its owned episode numbers.
type SeriesSeason struct {
	Season   int   `json:"season"`
	Episodes []int `json:"episodes"`
}

// Series aggregates the owned episodes of one TV show.
type Series struct {
	Title       string         `json:"title"`
	Year        int            `json:"year,omitempty"`
	TMDBID      int            `json:"tmdb_id,omitempty"`
	Episodes    int            `json:"episodes"`
	Seasons     []SeriesSeason `json:"seasons"`
	NextSeason  int            `json:"next_season,omitempty"`
	NextEpisode int            `json:"next_episode,omitempty"`
}

// Snapshot is the full library state returned by GET /api/library.
type Snapshot struct {
	Entries   []Entry  `json:"entries"`
	Series    []Series `json:"series"`
	Movies    int      `json:"movies"`
	Episodes  int      `json:"episodes"`
	ScannedAt string   `json:"scanned_at,omitempty"`
}

// ScanResult reports one library scan.
type ScanResult struct {
	Added     int    `json:"added"`
	Updated   int    `json:"updated"`
	Missing   int    `json:"missing"`
	ScannedAt string `json:"scanned_at"`
}

// Store is the SQLite-backed library index.
type Store struct {
	db *sql.DB
	mu sync.Mutex // serialises Scan; Add/Snapshot/NextUp rely on sql.DB
}

const upsertEntrySQL = `INSERT INTO library
  (disc_name, disc_hash, title, year, media_type, season, episode, tmdb_id, output_path, source, missing, moved_at, created_at)
VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
ON CONFLICT(output_path) DO UPDATE SET
  disc_name  = CASE WHEN excluded.disc_name <> '' THEN excluded.disc_name ELSE library.disc_name END,
  disc_hash  = CASE WHEN excluded.disc_hash <> '' THEN excluded.disc_hash ELSE library.disc_hash END,
  title      = excluded.title,
  year       = CASE WHEN excluded.year > 0 THEN excluded.year ELSE library.year END,
  media_type = excluded.media_type,
  season     = excluded.season,
  episode    = excluded.episode,
  tmdb_id    = CASE WHEN excluded.tmdb_id > 0 THEN excluded.tmdb_id ELSE library.tmdb_id END,
  source     = CASE WHEN excluded.source = 'rip' THEN 'rip' ELSE library.source END,
  missing    = 0,
  moved_at   = CASE WHEN excluded.moved_at <> '' THEN excluded.moved_at ELSE library.moved_at END`

// Open opens (creating when necessary) the SQLite database at path.
func Open(path string) (*Store, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		return nil, fmt.Errorf("library: create database dir: %w", err)
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, fmt.Errorf("library: open database: %w", err)
	}
	// A single connection keeps writes serialised and avoids SQLITE_BUSY.
	db.SetMaxOpenConns(1)

	if err := initSchema(db); err != nil {
		db.Close()
		return nil, err
	}
	return &Store{db: db}, nil
}

// schemaVersion is the version this build expects; the schema is stamped in
// PRAGMA user_version. Migrations are ordered, each upgrading from version
// N-1 to N. Append one entry per schema change — never edit or renumber an
// existing entry, and never delete one: an installed library.db may be at
// any earlier version.
const schemaVersion = 2

type migration struct {
	version int
	apply   func(*sql.DB) error
}

func migrations() []migration {
	return []migration{
		{1, migrateV1},
		{2, migrateV2},
	}
}

// migrateV1 creates the initial schema. All statements are idempotent so a
// pre-migration database (user_version 0 that already carries these tables
// from the old CREATE TABLE IF NOT EXISTS bootstrap) upgrades cleanly.
func migrateV1(db *sql.DB) error {
	stmts := []string{
		`CREATE TABLE IF NOT EXISTS library (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  disc_name TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL,
  year INTEGER NOT NULL DEFAULT 0,
  media_type TEXT NOT NULL,
  season INTEGER NOT NULL DEFAULT 0,
  episode INTEGER NOT NULL DEFAULT 0,
  tmdb_id INTEGER NOT NULL DEFAULT 0,
  output_path TEXT NOT NULL UNIQUE,
  source TEXT NOT NULL,
  missing INTEGER NOT NULL DEFAULT 0,
  moved_at TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
)`,
		`CREATE INDEX IF NOT EXISTS idx_library_title ON library(title COLLATE NOCASE)`,
		`CREATE INDEX IF NOT EXISTS idx_library_tmdb ON library(tmdb_id)`,
		`CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)`,
	}
	for _, stmt := range stmts {
		if _, err := db.Exec(stmt); err != nil {
			return fmt.Errorf("migrate v1: %w", err)
		}
	}
	return nil
}

// migrateV2 adds the disc_hash fingerprint column (issue #104): a SHA-256
// over the disc's TINFO metadata, used to detect already-ripped discs whose
// disc_name is generic (e.g. "DVD_VIDEO").
func migrateV2(db *sql.DB) error {
	stmts := []string{
		`ALTER TABLE library ADD COLUMN disc_hash TEXT NOT NULL DEFAULT ''`,
		`CREATE INDEX IF NOT EXISTS idx_library_disc_hash ON library(disc_hash)`,
	}
	for _, stmt := range stmts {
		if _, err := db.Exec(stmt); err != nil {
			return fmt.Errorf("migrate v2: %w", err)
		}
	}
	return nil
}

// initSchema applies pragmas, then every migration above the stamped
// user_version, stamping after each so an interrupted upgrade restarts
// correctly.
func initSchema(db *sql.DB) error {
	for _, pragma := range []string{`PRAGMA journal_mode=WAL`, `PRAGMA busy_timeout=5000`} {
		if _, err := db.Exec(pragma); err != nil {
			return fmt.Errorf("library: %s: %w", pragma, err)
		}
	}

	var current int
	if err := db.QueryRow(`PRAGMA user_version`).Scan(&current); err != nil {
		return fmt.Errorf("library: read schema version: %w", err)
	}
	if current > schemaVersion {
		return fmt.Errorf("library: database schema version %d is newer than this build supports (%d)", current, schemaVersion)
	}

	for _, m := range migrations() {
		if m.version <= current {
			continue
		}
		if err := m.apply(db); err != nil {
			return fmt.Errorf("library: migrate to v%d: %w", m.version, err)
		}
		if _, err := db.Exec(fmt.Sprintf(`PRAGMA user_version=%d`, m.version)); err != nil {
			return fmt.Errorf("library: stamp schema version %d: %w", m.version, err)
		}
		logger.Info("Library schema migrated", "from_version", current, "to_version", m.version)
		current = m.version
	}
	return nil
}

// Close closes the underlying database.
func (s *Store) Close() error {
	if s == nil || s.db == nil {
		return nil
	}
	return s.db.Close()
}

// Add upserts one entry keyed by output_path, keeping richer existing values.
func (s *Store) Add(ctx context.Context, e Entry) error {
	if strings.TrimSpace(e.Title) == "" || e.OutputPath == "" {
		return fmt.Errorf("library: entry needs title and output_path")
	}
	if e.Source == "" {
		e.Source = "scan"
	}
	if e.CreatedAt == "" {
		e.CreatedAt = time.Now().UTC().Format(time.RFC3339)
	}
	_, err := s.db.ExecContext(ctx, upsertEntrySQL,
		e.DiscName, e.DiscHash, e.Title, e.Year, e.MediaType, e.Season, e.Episode,
		e.TMDBID, e.OutputPath, e.Source, 0, e.MovedAt, e.CreatedAt)
	if err != nil {
		return fmt.Errorf("library: upsert entry %q: %w", e.OutputPath, err)
	}
	return nil
}

// Snapshot returns every entry plus aggregated series and counters.
func (s *Store) Snapshot(ctx context.Context) (Snapshot, error) {
	snap := Snapshot{Entries: make([]Entry, 0)}

	rows, err := s.db.QueryContext(ctx, `SELECT id, disc_name, disc_hash, title, year, media_type, season, episode, tmdb_id, output_path, source, missing, moved_at, created_at FROM library ORDER BY title COLLATE NOCASE, season, episode`)
	if err != nil {
		return snap, fmt.Errorf("library: query entries: %w", err)
	}
	defer rows.Close()

	var tv []Entry
	for rows.Next() {
		var (
			e       Entry
			missing int
		)
		if err := rows.Scan(&e.ID, &e.DiscName, &e.DiscHash, &e.Title, &e.Year, &e.MediaType,
			&e.Season, &e.Episode, &e.TMDBID, &e.OutputPath, &e.Source,
			&missing, &e.MovedAt, &e.CreatedAt); err != nil {
			return snap, fmt.Errorf("library: scan entry: %w", err)
		}
		e.Missing = missing != 0
		snap.Entries = append(snap.Entries, e)
		if !e.Missing && e.MediaType == "tv" {
			tv = append(tv, e)
		}
	}
	if err := rows.Err(); err != nil {
		return snap, fmt.Errorf("library: read entries: %w", err)
	}

	var scannedAt sql.NullString
	err = s.db.QueryRowContext(ctx, `SELECT value FROM meta WHERE key='scanned_at'`).Scan(&scannedAt)
	if err != nil && err != sql.ErrNoRows {
		return snap, fmt.Errorf("library: read scanned_at: %w", err)
	}
	if scannedAt.Valid {
		snap.ScannedAt = scannedAt.String
	}

	for _, e := range snap.Entries {
		if e.Missing {
			continue
		}
		switch e.MediaType {
		case "movie":
			snap.Movies++
		case "tv":
			snap.Episodes++
		}
	}
	snap.Series = buildSeries(tv)
	return snap, nil
}

// buildSeries groups non-missing TV entries by lowercased title.
func buildSeries(entries []Entry) []Series {
	type agg struct {
		title   string
		year    int
		tmdbID  int
		present map[int]map[int]bool
		count   int
	}
	var order []string
	groups := make(map[string]*agg)
	for _, e := range entries {
		key := strings.ToLower(strings.TrimSpace(e.Title))
		g, ok := groups[key]
		if !ok {
			g = &agg{present: make(map[int]map[int]bool)}
			groups[key] = g
			order = append(order, key)
		}
		g.count++
		if g.title == "" {
			g.title = e.Title
		}
		if g.year == 0 {
			g.year = e.Year
		}
		if g.tmdbID == 0 {
			g.tmdbID = e.TMDBID
		}
		if g.present[e.Season] == nil {
			g.present[e.Season] = make(map[int]bool)
		}
		g.present[e.Season][e.Episode] = true
	}

	series := make([]Series, 0, len(groups))
	for _, key := range order {
		g := groups[key]
		seasons := make([]int, 0, len(g.present))
		for sN := range g.present {
			seasons = append(seasons, sN)
		}
		sort.Ints(seasons)
		var seasonsOut []SeriesSeason
		for _, sN := range seasons {
			eps := make([]int, 0, len(g.present[sN]))
			for ep := range g.present[sN] {
				eps = append(eps, ep)
			}
			sort.Ints(eps)
			seasonsOut = append(seasonsOut, SeriesSeason{Season: sN, Episodes: eps})
		}
		if seasonsOut == nil {
			seasonsOut = []SeriesSeason{}
		}
		out := Series{
			Title:    g.title,
			Year:     g.year,
			TMDBID:   g.tmdbID,
			Episodes: g.count,
			Seasons:  seasonsOut,
		}
		// Next up: highest season > 0 and one past its highest episode.
		for i := len(seasonsOut) - 1; i >= 0; i-- {
			ss := seasonsOut[i]
			if ss.Season <= 0 {
				continue
			}
			out.NextSeason = ss.Season
			highest := 0
			for _, ep := range ss.Episodes {
				if ep > highest {
					highest = ep
				}
			}
			out.NextEpisode = highest + 1
			break
		}
		series = append(series, out)
	}
	sort.SliceStable(series, func(i, j int) bool {
		return strings.ToLower(series[i].Title) < strings.ToLower(series[j].Title)
	})
	return series
}

// NextUp returns the next season/episode to rip for a title. ok is false when
// no stored episode carries a season number > 0 for that title.
func (s *Store) NextUp(ctx context.Context, title string, tmdbID, seasonHint int) (season, episode int, ok bool) {
	rows, err := s.db.QueryContext(ctx, `SELECT title, tmdb_id, season, episode FROM library WHERE media_type='tv' AND missing=0`)
	if err != nil {
		return 0, 0, false
	}
	defer rows.Close()

	type row struct {
		title   string
		tmdbID  int
		season  int
		episode int
	}
	var matched []row
	for rows.Next() {
		var r row
		if err := rows.Scan(&r.title, &r.tmdbID, &r.season, &r.episode); err != nil {
			return 0, 0, false
		}
		matched = append(matched, r)
	}
	if err := rows.Err(); err != nil {
		return 0, 0, false
	}

	want := strings.TrimSpace(title)
	var filtered []row
	for _, r := range matched {
		if tmdbID > 0 && r.tmdbID > 0 {
			if r.tmdbID == tmdbID {
				filtered = append(filtered, r)
			}
			continue
		}
		if strings.EqualFold(strings.TrimSpace(r.title), want) && want != "" {
			filtered = append(filtered, r)
		}
	}

	if seasonHint > 0 {
		highest := 0
		found := false
		for _, r := range filtered {
			if r.season == seasonHint && r.episode > highest {
				highest = r.episode
				found = true
			}
		}
		if !found {
			return seasonHint, 1, true
		}
		return seasonHint, highest + 1, true
	}

	highestSeason := 0
	for _, r := range filtered {
		if r.season > highestSeason {
			highestSeason = r.season
		}
	}
	if highestSeason == 0 {
		return 0, 0, false
	}
	highestEp := 0
	for _, r := range filtered {
		if r.season == highestSeason && r.episode > highestEp {
			highestEp = r.episode
		}
	}
	return highestSeason, highestEp + 1, true
}

// LastRip is the most recently moved/recorded library entry.
type LastRip struct {
	Title     string `json:"title"`
	Year      int    `json:"year"`
	MediaType string `json:"media_type"`
	Season    int    `json:"season"`
	Episode   int    `json:"episode"`
	TMDBID    int    `json:"tmdb_id"`
}

// LastRip returns the most recently recorded library entry (by moved_at, then
// created_at). ok is false when the index holds no entries.
func (s *Store) LastRip(ctx context.Context) (LastRip, bool) {
	var lr LastRip
	row := s.db.QueryRowContext(ctx, `SELECT title, year, media_type, season, episode, tmdb_id FROM library ORDER BY moved_at DESC, created_at DESC LIMIT 1`)
	if err := row.Scan(&lr.Title, &lr.Year, &lr.MediaType, &lr.Season, &lr.Episode, &lr.TMDBID); err != nil {
		return LastRip{}, false
	}
	return lr, true
}

// RipMatches reports library entries whose disc_name matches name
// case-insensitively (SQLite LIKE is case-insensitive for ASCII). The most
// recent entry per distinct output_path is returned, newest first.
func (s *Store) RipMatches(ctx context.Context, name string) ([]Entry, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return nil, nil
	}
	rows, err := s.db.QueryContext(ctx, `SELECT id, disc_name, disc_hash, title, year, media_type, season, episode, tmdb_id, output_path, source, missing, moved_at, created_at FROM library WHERE disc_name LIKE ? ESCAPE '\' AND missing = 0 ORDER BY moved_at DESC, created_at DESC`, escapeLike(name))
	if err != nil {
		return nil, fmt.Errorf("library: rip matches: %w", err)
	}
	defer rows.Close()
	out := make([]Entry, 0)
	seen := make(map[string]bool)
	for rows.Next() {
		var (
			e       Entry
			missing int
		)
		if err := rows.Scan(&e.ID, &e.DiscName, &e.DiscHash, &e.Title, &e.Year, &e.MediaType, &e.Season, &e.Episode, &e.TMDBID, &e.OutputPath, &e.Source, &missing, &e.MovedAt, &e.CreatedAt); err != nil {
			return nil, fmt.Errorf("library: scan rip matches: %w", err)
		}
		e.Missing = missing != 0
		if seen[e.OutputPath] {
			continue
		}
		seen[e.OutputPath] = true
		out = append(out, e)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("library: read rip matches: %w", err)
	}
	return out, nil
}

// HashMatches reports library entries whose disc_hash equals hash exactly
// (missing entries excluded), newest first. This is the reliable duplicate
// signal when the disc_name is generic.
func (s *Store) HashMatches(ctx context.Context, hash string) ([]Entry, error) {
	hash = strings.TrimSpace(hash)
	if hash == "" {
		return nil, nil
	}
	rows, err := s.db.QueryContext(ctx, `SELECT id, disc_name, disc_hash, title, year, media_type, season, episode, tmdb_id, output_path, source, missing, moved_at, created_at FROM library WHERE disc_hash = ? AND missing = 0 ORDER BY moved_at DESC, created_at DESC`, hash)
	if err != nil {
		return nil, fmt.Errorf("library: hash matches: %w", err)
	}
	defer rows.Close()
	out := make([]Entry, 0)
	seen := make(map[string]bool)
	for rows.Next() {
		var (
			e       Entry
			missing int
		)
		if err := rows.Scan(&e.ID, &e.DiscName, &e.DiscHash, &e.Title, &e.Year, &e.MediaType, &e.Season, &e.Episode, &e.TMDBID, &e.OutputPath, &e.Source, &missing, &e.MovedAt, &e.CreatedAt); err != nil {
			return nil, fmt.Errorf("library: scan hash matches: %w", err)
		}
		e.Missing = missing != 0
		if seen[e.OutputPath] {
			continue
		}
		seen[e.OutputPath] = true
		out = append(out, e)
	}
	return out, rows.Err()
}

// escapeLike escapes LIKE metacharacters so a disc name matches literally.
func escapeLike(s string) string {
	s = strings.ReplaceAll(s, `\`, `\\`)
	s = strings.ReplaceAll(s, `%`, `\%`)
	s = strings.ReplaceAll(s, `_`, `\_`)
	return s
}

// Reset clears the library index (entries, meta, auto-increment counter).
// It never touches the database file itself or any media files.
func (s *Store) Reset(ctx context.Context) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("library: begin reset: %w", err)
	}
	for _, stmt := range []string{
		`DELETE FROM library`,
		`DELETE FROM meta`,
		`DELETE FROM sqlite_sequence WHERE name = 'library'`,
	} {
		if _, err := tx.ExecContext(ctx, stmt); err != nil {
			tx.Rollback()
			return fmt.Errorf("library: reset: %w", err)
		}
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("library: commit reset: %w", err)
	}
	logger.Info("Library database cleared", "entries", "all")
	return nil
}
