-- Create files table
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    size INTEGER,
    duration INTEGER,
    hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Scanned',
    suggested_title TEXT,
    suggested_year INTEGER,
    confidence REAL,
    canonical_title TEXT,
    canonical_year INTEGER,
    tmdb_id INTEGER,
    content_type TEXT DEFAULT 'unknown',
    media_type TEXT DEFAULT 'movie',
    series_name TEXT,
    season_number INTEGER,
    episode_number INTEGER,
    episode_title TEXT,
    source TEXT,
    subtitle_text TEXT,
    duplicate_path TEXT,
    reviewed INTEGER DEFAULT 0,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_files_status ON files(status);
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_files_series_season ON files(series_name, season_number);
CREATE INDEX IF NOT EXISTS idx_files_confidence ON files(confidence);

-- Create schema version table
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT DEFAULT CURRENT_TIMESTAMP
);