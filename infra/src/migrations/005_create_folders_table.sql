-- ADR-026: Folder-Based Pipeline — new folders table for Organizer window

CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    suggested_name TEXT,
    suggested_year INTEGER,
    confidence REAL,
    tmdb_id INTEGER,
    media_type TEXT NOT NULL DEFAULT 'Movie',
    file_count INTEGER NOT NULL DEFAULT 0,
    main_file TEXT,
    extra_files TEXT,              -- JSON array
    status TEXT NOT NULL DEFAULT 'Scanned',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_folders_status ON folders(status);
