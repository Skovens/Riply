-- Add CHECK constraints for status values
-- SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so we need to recreate tables

-- Recreate files table with CHECK constraint
CREATE TABLE files_new (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    size INTEGER,
    duration INTEGER,
    hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Scanned' CHECK(status IN ('Scanned', 'Analyzed', 'Review', 'Confirmed', 'Ripped', 'Moved', 'Failed')),
    suggested_title TEXT,
    suggested_year INTEGER,
    confidence REAL,
    canonical_title TEXT,
    canonical_year INTEGER,
    tmdb_id INTEGER,
    content_type TEXT DEFAULT 'unknown' CHECK(content_type IN ('unknown', 'video', 'audio', 'subtitle')),
    media_type TEXT DEFAULT 'movie' CHECK(media_type IN ('movie', 'tv', 'music')),
    series_name TEXT,
    season_number INTEGER,
    episode_number INTEGER,
    episode_title TEXT,
    source TEXT,
    subtitle_text TEXT,
    duplicate_path TEXT,
    title_group_id TEXT,
    reviewed INTEGER DEFAULT 0 CHECK(reviewed IN (0, 1)),
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Copy data
INSERT INTO files_new SELECT * FROM files;

-- Drop old table and rename
DROP TABLE files;
ALTER TABLE files_new RENAME TO files;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_files_status ON files(status);
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_files_series_season ON files(series_name, season_number);
CREATE INDEX IF NOT EXISTS idx_files_confidence ON files(confidence);
CREATE INDEX IF NOT EXISTS idx_files_title_group ON files(title_group_id);
CREATE INDEX IF NOT EXISTS idx_files_status_title ON files(status, canonical_title);

-- Recreate title_groups table with CHECK constraint
CREATE TABLE title_groups_new (
    id TEXT PRIMARY KEY,
    source_directory TEXT NOT NULL,
    title TEXT NOT NULL,
    year INTEGER,
    tmdb_id INTEGER,
    media_type TEXT NOT NULL DEFAULT 'movie' CHECK(media_type IN ('movie', 'tv', 'music')),
    series_name TEXT,
    status TEXT NOT NULL DEFAULT 'New' CHECK(status IN ('New', 'Matched', 'Confirmed', 'Moved'))
);

-- Copy data
INSERT INTO title_groups_new SELECT * FROM title_groups;

-- Drop old table and rename
DROP TABLE title_groups;
ALTER TABLE title_groups_new RENAME TO title_groups;

-- Recreate index
CREATE INDEX IF NOT EXISTS idx_title_groups_status ON title_groups(status);
