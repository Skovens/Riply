package library

import (
	"context"
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func mkFile(t *testing.T, path string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("x"), 0644); err != nil {
		t.Fatal(err)
	}
}

// tv_shows_dir (#122): Scan must cover both roots in one pass — entries from
// each root are indexed, and a file deleted from either root is flagged
// missing while entries under the other root stay untouched.
func TestScanTwoRoots(t *testing.T) {
	dir := t.TempDir()
	movies := filepath.Join(dir, "movies")
	tv := filepath.Join(dir, "tvshows")
	mkFile(t, filepath.Join(movies, "Inception (2010)", "Inception.mkv"))
	s01e01 := filepath.Join(tv, "KLOVN (2010)", "Season 1", "KLOVN - S01E01 - Pilot.mkv")
	mkFile(t, s01e01)

	st := openStore(t, filepath.Join(dir, "library.db"))
	ctx := context.Background()

	res, err := st.Scan(ctx, movies, tv)
	if err != nil {
		t.Fatal(err)
	}
	if res.Added != 2 || res.Missing != 0 {
		t.Fatalf("two-root scan = %+v, want added 2", res)
	}
	snap, err := st.Snapshot(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if snap.Movies != 1 || snap.Episodes != 1 {
		t.Errorf("counters: movies=%d episodes=%d, want 1/1", snap.Movies, snap.Episodes)
	}

	// Delete from the TV root: that entry goes missing, the movie entry
	// (other root) is untouched.
	if err := os.Remove(s01e01); err != nil {
		t.Fatal(err)
	}
	res, err = st.Scan(ctx, movies, tv)
	if err != nil {
		t.Fatal(err)
	}
	if res.Missing != 1 {
		t.Errorf("two-root missing scan = %+v, want missing 1", res)
	}
	snap, err = st.Snapshot(ctx)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range snap.Entries {
		if e.Title == "Inception" && e.Missing {
			t.Error("movie entry flagged missing while its root was scanned and file present")
		}
	}

	// A nonexistent optional root (tv_shows_dir not created yet) is skipped.
	if _, err := st.Scan(ctx, movies, filepath.Join(dir, "not-created")); err != nil {
		t.Errorf("scan with nonexistent optional root: %v", err)
	}
}

func TestScanAndSnapshot(t *testing.T) {
	dir := t.TempDir()
	movies := filepath.Join(dir, "movies")
	klov1 := filepath.Join(movies, "KLOVN (2010)", "Season 1")
	klov2 := filepath.Join(movies, "KLOVN (2010)", "Season 2")
	mkFile(t, filepath.Join(klov1, "KLOVN - S01E01 - Pilot.mkv"))
	mkFile(t, filepath.Join(klov1, "KLOVN - S01E02 - Second.mkv"))
	s02e01 := filepath.Join(klov2, "KLOVN - S02E01 - Third.mkv")
	mkFile(t, s02e01)
	mkFile(t, filepath.Join(movies, "Inception (2010)", "Inception.mkv"))

	st := openStore(t, filepath.Join(dir, "library.db"))
	ctx := context.Background()

	res, err := st.Scan(ctx, movies)
	if err != nil {
		t.Fatal(err)
	}
	if res.Added != 4 || res.Updated != 0 || res.Missing != 0 {
		t.Fatalf("first scan = %+v, want added 4", res)
	}

	snap, err := st.Snapshot(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if snap.Movies != 1 || snap.Episodes != 3 {
		t.Errorf("counters: movies=%d episodes=%d, want 1/3", snap.Movies, snap.Episodes)
	}
	if len(snap.Series) != 1 {
		t.Fatalf("series count = %d, want 1", len(snap.Series))
	}
	s := snap.Series[0]
	if s.Title != "KLOVN" || s.NextSeason != 2 || s.NextEpisode != 2 {
		t.Errorf("series next-up = %+v, want KLOVN season 2 episode 2", s)
	}

	res2, err := st.Scan(ctx, movies)
	if err != nil {
		t.Fatal(err)
	}
	if res2.Added != 0 || res2.Updated != 4 {
		t.Errorf("rescan = %+v, want updated 4", res2)
	}

	if err := os.Remove(s02e01); err != nil {
		t.Fatal(err)
	}
	res3, err := st.Scan(ctx, movies)
	if err != nil {
		t.Fatal(err)
	}
	if res3.Missing != 1 {
		t.Errorf("missing-flag scan = %+v, want missing 1", res3)
	}
	snap3, _ := st.Snapshot(ctx)
	if len(snap3.Series) != 1 || snap3.Series[0].NextSeason != 1 || snap3.Series[0].NextEpisode != 3 {
		t.Errorf("after deletion next-up = %+v, want season 1 episode 3", snap3.Series)
	}
	var missing int
	for _, e := range snap3.Entries {
		if e.Missing {
			missing++
		}
	}
	if missing != 1 {
		t.Errorf("missing entries = %d, want 1", missing)
	}
}

func TestNextUp(t *testing.T) {
	dir := t.TempDir()
	st := openStore(t, filepath.Join(dir, "library.db"))
	ctx := context.Background()

	add := func(title string, season, episode int) {
		t.Helper()
		err := st.Add(ctx, Entry{
			Title: title, MediaType: "tv", Season: season, Episode: episode,
			OutputPath: filepath.Join(dir, title+"-S"+string(rune('0'+season))+"E"+string(rune('0'+episode))+".mkv"),
		})
		if err != nil {
			t.Fatal(err)
		}
	}
	add("KLOVN", 2, 1)

	if season, episode, ok := st.NextUp(ctx, "KLOVN", 0, 2); !ok || season != 2 || episode != 2 {
		t.Errorf("NextUp hint 2 = (%d,%d,%v), want (2,2,true)", season, episode, ok)
	}
	if season, episode, ok := st.NextUp(ctx, "KLOVN", 0, 0); !ok || season != 2 || episode != 2 {
		t.Errorf("NextUp hint 0 = (%d,%d,%v), want (2,2,true)", season, episode, ok)
	}
	if season, episode, ok := st.NextUp(ctx, "Unknown", 0, 3); !ok || season != 3 || episode != 1 {
		t.Errorf("NextUp unknown hint 3 = (%d,%d,%v), want (3,1,true)", season, episode, ok)
	}
	if _, _, ok := st.NextUp(ctx, "Unknown", 0, 0); ok {
		t.Error("NextUp unknown hint 0 ok = true, want false")
	}
}

func TestPersistence(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "library.db")
	st := openStore(t, path)
	ctx := context.Background()

	out := filepath.Join(dir, "movie.mkv")
	if err := st.Add(ctx, Entry{Title: "Heat", Year: 1995, MediaType: "movie", OutputPath: out}); err != nil {
		t.Fatal(err)
	}
	first, _ := st.Snapshot(ctx)
	if len(first.Entries) != 1 {
		t.Fatalf("entries = %d, want 1", len(first.Entries))
	}
	id := first.Entries[0].ID

	if err := st.Close(); err != nil {
		t.Fatal(err)
	}
	st2 := openStore(t, path)
	defer st2.Close()
	second, err := st2.Snapshot(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(second.Entries) != 1 || second.Entries[0].ID != id || second.Entries[0].Title != "Heat" {
		t.Errorf("after reopen = %+v, want one entry with the same ID", second.Entries)
	}
}

func TestReset(t *testing.T) {
	dir := t.TempDir()
	st := openStore(t, filepath.Join(dir, "library.db"))
	ctx := context.Background()

	for _, title := range []string{"A", "B"} {
		err := st.Add(ctx, Entry{Title: title, MediaType: "movie", OutputPath: filepath.Join(dir, title+".mkv"), Source: "rip"})
		if err != nil {
			t.Fatal(err)
		}
	}
	// Scan stamps meta.scanned_at, so the assertion after Reset proves that reset
	// clears the meta table and not just the library table.
	if _, err := st.Scan(ctx, dir); err != nil {
		t.Fatal(err)
	}
	snap, _ := st.Snapshot(ctx)
	if len(snap.Entries) < 2 {
		t.Fatalf("entries before reset = %d, want >= 2", len(snap.Entries))
	}
	if snap.ScannedAt == "" {
		t.Fatal("scanned_at not set by Scan; the post-reset assertion would be vacuous")
	}

	if err := st.Reset(ctx); err != nil {
		t.Fatal(err)
	}
	snap, _ = st.Snapshot(ctx)
	if len(snap.Entries) != 0 {
		t.Errorf("entries after reset = %d, want 0", len(snap.Entries))
	}
	if snap.ScannedAt != "" {
		t.Errorf("scanned_at after reset = %q, want empty", snap.ScannedAt)
	}

	if err := st.Add(ctx, Entry{Title: "C", MediaType: "movie", OutputPath: filepath.Join(dir, "C.mkv")}); err != nil {
		t.Fatal(err)
	}
	snap, _ = st.Snapshot(ctx)
	if len(snap.Entries) != 1 || snap.Entries[0].ID != 1 {
		t.Errorf("first id after reset = %+v, want ID 1 (sequence cleared)", snap.Entries)
	}
}

func TestLastRip(t *testing.T) {
	dir := t.TempDir()
	st := openStore(t, filepath.Join(dir, "library.db"))
	ctx := context.Background()

	if _, ok := st.LastRip(ctx); ok {
		t.Fatal("LastRip on empty store returned ok")
	}

	// Entries share moved_at; created_at DESC must still return exactly one row.
	for ep := 1; ep <= 10; ep++ {
		if err := st.Add(ctx, Entry{
			Title: "KLOVN", MediaType: "tv", Season: 1, Episode: ep,
			DiscName:   "KLOVN S01",
			OutputPath: fmt.Sprintf("/x/KLOVN S01/E%d.mkv", ep), Source: "rip",
			MovedAt: "2026-01-01T00:00:00Z",
		}); err != nil {
			t.Fatal(err)
		}
	}
	// A newer entry must win.
	if err := st.Add(ctx, Entry{
		Title: "Inception", MediaType: "movie",
		OutputPath: "/x/Inception (2010).mkv", Source: "rip",
		MovedAt: "2026-06-01T00:00:00Z",
	}); err != nil {
		t.Fatal(err)
	}

	lr, ok := st.LastRip(ctx)
	if !ok {
		t.Fatal("LastRip returned !ok with entries present")
	}
	if lr.Title != "Inception" || lr.MediaType != "movie" {
		t.Errorf("LastRip = %+v, want Inception/movie (newest moved_at)", lr)
	}
}

func TestRipMatches(t *testing.T) {
	dir := t.TempDir()
	st := openStore(t, filepath.Join(dir, "library.db"))
	ctx := context.Background()

	for ep := 1; ep <= 10; ep++ {
		if err := st.Add(ctx, Entry{
			Title: "KLOVN", MediaType: "tv", Season: 1, Episode: ep,
			DiscName:   "KLOVN S01",
			OutputPath: fmt.Sprintf("/x/KLOVN S01/E%d.mkv", ep), Source: "rip",
		}); err != nil {
			t.Fatal(err)
		}
	}
	if err := st.Add(ctx, Entry{
		Title: "The Mask 2", MediaType: "movie",
		DiscName: "MASK 2", OutputPath: "/x/Mask 2/Mask 2.mkv", Source: "rip",
	}); err != nil {
		t.Fatal(err)
	}

	// Case-insensitive disc-name match.
	matches, err := st.RipMatches(ctx, "klovn s01")
	if err != nil {
		t.Fatal(err)
	}
	if len(matches) != 10 {
		t.Errorf("RipMatches(klovn s01) = %d entries, want 10", len(matches))
	}

	// No match, and empty/blank queries are no-ops.
	if m, err := st.RipMatches(ctx, "other"); err != nil || len(m) != 0 {
		t.Errorf("RipMatches(other) = %d entries, err %v, want 0, nil", len(m), err)
	}
	if m, err := st.RipMatches(ctx, "  "); err != nil || len(m) != 0 {
		t.Errorf("RipMatches(blank) = %d entries, err %v, want 0, nil", len(m), err)
	}

	// LIKE metacharacters in the disc name match literally, not as wildcards.
	if m, err := st.RipMatches(ctx, "%"); err != nil || len(m) != 0 {
		t.Errorf(`RipMatches("%%") = %d entries, err %v, want 0 (no wildcard match)`, len(m), err)
	}

	// Entries flagged missing by a later scan are excluded.
	if _, err := st.db.ExecContext(ctx, `UPDATE library SET missing=1 WHERE episode=1 AND disc_name='KLOVN S01'`); err != nil {
		t.Fatal(err)
	}
	if m, err := st.RipMatches(ctx, "KLOVN S01"); err != nil || len(m) != 9 {
		t.Errorf("RipMatches after flagging one missing = %d entries, err %v, want 9", len(m), err)
	}
}

func openStore(t *testing.T, path string) *Store {
	t.Helper()
	st, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	return st
}

// L8: a pre-migration database (old bootstrap: tables present,
// user_version left at 0) must upgrade in place at startup: data survives,
// user_version reads 1, and the migrated store works.
func TestOpenMigratesV0Database(t *testing.T) {
	path := filepath.Join(t.TempDir(), "library.db")

	// Build the old-shape DB with raw SQL, exactly as the pre-migration
	// bootstrap did: tables + indexes, no PRAGMA user_version.
	raw, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	for _, stmt := range []string{
		`CREATE TABLE library (
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
		`CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)`,
		`INSERT INTO library (title, media_type, season, episode, output_path, source, created_at)
         VALUES ('Old Movie', 'movie', 0, 0, '/lib/Old Movie (2001)/Old Movie.mkv', 'scan', '2024-01-01T00:00:00Z')`,
	} {
		if _, err := raw.Exec(stmt); err != nil {
			t.Fatal(err)
		}
	}
	var v int
	if err := raw.QueryRow(`PRAGMA user_version`).Scan(&v); err != nil {
		t.Fatal(err)
	}
	if v != 0 {
		t.Fatalf("fixture user_version = %d, want 0", v)
	}
	raw.Close()

	// Opening with the Store must migrate to the current schema version.
	st := openStore(t, path)
	if err := st.db.QueryRow(`PRAGMA user_version`).Scan(&v); err != nil {
		t.Fatal(err)
	}
	if v != schemaVersion {
		t.Errorf("after Open user_version = %d, want %d", v, schemaVersion)
	}
	snap, err := st.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Entries) != 1 || snap.Entries[0].Title != "Old Movie" {
		t.Errorf("pre-existing row lost during migration: %+v", snap.Entries)
	}
	if err := st.Add(context.Background(), Entry{Title: "New", MediaType: "movie", OutputPath: "/lib/New.mkv", Source: "rip", CreatedAt: "2025-01-01T00:00:00Z"}); err != nil {
		t.Errorf("Add after migration = %v", err)
	}
}

// Re-opening an already-migrated DB is a no-op (no re-apply, version stable).
func TestOpenIdempotentOnCurrentVersion(t *testing.T) {
	path := filepath.Join(t.TempDir(), "library.db")
	st := openStore(t, path)
	if err := st.Add(context.Background(), Entry{Title: "A", MediaType: "movie", OutputPath: "/lib/A.mkv", Source: "rip", CreatedAt: "2025-01-01T00:00:00Z"}); err != nil {
		t.Fatal(err)
	}
	st.Close()

	st2 := openStore(t, path)
	var v int
	if err := st2.db.QueryRow(`PRAGMA user_version`).Scan(&v); err != nil {
		t.Fatal(err)
	}
	if v != schemaVersion {
		t.Errorf("user_version after reopen = %d, want %d", v, schemaVersion)
	}
	snap, err := st2.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Entries) != 1 {
		t.Errorf("entries after reopen = %d, want 1", len(snap.Entries))
	}
}

func TestHashMatchesAndMigrationV2(t *testing.T) {
	store := openStore(t, filepath.Join(t.TempDir(), "library.db"))
	ctx := context.Background()

	// Legacy entry (hash unknown) + two entries with hashes.
	if err := store.Add(ctx, Entry{Title: "Old Rip", MediaType: "movie", OutputPath: "/m/Old Rip/Inception.mkv"}); err != nil {
		t.Fatal(err)
	}
	if err := store.Add(ctx, Entry{Title: "Hashed", MediaType: "movie", DiscHash: "abc123", OutputPath: "/m/Hashed/main.mkv"}); err != nil {
		t.Fatal(err)
	}
	if err := store.Add(ctx, Entry{Title: "Other", MediaType: "movie", DiscHash: "def456", OutputPath: "/m/Other/main.mkv"}); err != nil {
		t.Fatal(err)
	}

	// Exact-hash match only.
	matches, err := store.HashMatches(ctx, "abc123")
	if err != nil {
		t.Fatal(err)
	}
	if len(matches) != 1 || matches[0].Title != "Hashed" {
		t.Fatalf("hash matches = %+v, want exactly [Hashed]", matches)
	}

	// Empty hash never matches anything.
	if m, err := store.HashMatches(ctx, ""); err != nil || len(m) != 0 {
		t.Fatalf("empty hash matched: %v %v", m, err)
	}

	// Entries survive a Snapshot round-trip with their hash.
	snap, err := store.Snapshot(ctx)
	if err != nil {
		t.Fatal(err)
	}
	hashes := map[string]string{}
	for _, e := range snap.Entries {
		hashes[e.Title] = e.DiscHash
	}
	if hashes["Hashed"] != "abc123" || hashes["Old Rip"] != "" {
		t.Fatalf("snapshot hashes = %v", hashes)
	}
}

func TestUpsertPreservesExistingHash(t *testing.T) {
	store := openStore(t, filepath.Join(t.TempDir(), "library.db"))
	ctx := context.Background()

	// Rip writes the hash...
	if err := store.Add(ctx, Entry{Title: "X", MediaType: "movie", DiscHash: "h1", Source: "rip", OutputPath: "/m/X/main.mkv"}); err != nil {
		t.Fatal(err)
	}
	// ...a later scan-derived upsert without a hash must not clobber it.
	if err := store.Add(ctx, Entry{Title: "X", MediaType: "movie", Source: "scan", OutputPath: "/m/X/main.mkv"}); err != nil {
		t.Fatal(err)
	}
	matches, err := store.HashMatches(ctx, "h1")
	if err != nil {
		t.Fatal(err)
	}
	if len(matches) != 1 {
		t.Fatalf("hash lost after scan upsert: %v", matches)
	}
}

func TestMigrateV2AddsColumnToV1Database(t *testing.T) {
	// Build a v1-shaped DB (user_version = 1, no disc_hash column) with raw SQL.
	path := filepath.Join(t.TempDir(), "library.db")
	raw, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	for _, stmt := range []string{
		`CREATE TABLE library (
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
		`INSERT INTO library (title, media_type, output_path, source, created_at) VALUES ('Legacy', 'movie', '/m/L/a.mkv', 'rip', '2024-01-01T00:00:00Z')`,
		`PRAGMA user_version=1`,
	} {
		if _, err := raw.Exec(stmt); err != nil {
			t.Fatal(err)
		}
	}
	if err := raw.Close(); err != nil {
		t.Fatal(err)
	}

	store := openStore(t, path)
	if _, err := store.HashMatches(context.Background(), "whatever"); err != nil {
		t.Fatalf("hash query on migrated db: %v", err)
	}
}

// #103: a corrupt/unreadable scan root must not crash or wedge the store.
func TestScanUnreadableRootAndUnreadableSubdir(t *testing.T) {
	store := openStore(t, filepath.Join(t.TempDir(), "library.db"))
	ctx := context.Background()

	// Not a directory / missing → explicit error.
	if _, err := store.Scan(ctx, filepath.Join(t.TempDir(), "nope")); err == nil {
		t.Error("Scan on missing dir succeeded, want error")
	}

	// Unreadable subdirectory: walk must warn + skip, not fail.
	root := t.TempDir()
	movies := filepath.Join(root, "movies")
	readable := filepath.Join(movies, "Good Movie (2001)")
	unreadable := filepath.Join(movies, "Locked Movie (2002)")
	if err := os.MkdirAll(readable, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(unreadable, 0o755); err != nil {
		t.Fatal(err)
	}
	mkFile(t, filepath.Join(readable, "Good Movie (2001).mkv"))
	if err := os.Chmod(unreadable, 0o000); err != nil {
		t.Fatal(err)
	}
	if os.Getuid() == 0 {
		t.Skip("running as root: unreadable dir is still readable")
	}
	defer os.Chmod(unreadable, 0o755)

	res, err := store.Scan(ctx, movies)
	if err != nil {
		t.Fatalf("Scan failed on unreadable subdir: %v", err)
	}
	if res.Added != 1 {
		t.Errorf("Added = %d, want 1 (readable entry scanned, unreadable skipped)", res.Added)
	}
}

	if _, err := raw.Exec(`CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)`); err != nil {
		t.Fatal(err)
	}
	if _, err := raw.Exec(`CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)`); err != nil {
		t.Fatal(err)
	}