-- Session state: persists per-drive rip history for auto-suggest and session memory.
-- Survives server restarts. Resettable from Settings.

CREATE TABLE IF NOT EXISTS session_state (
    drive_index INTEGER PRIMARY KEY,
    last_title TEXT NOT NULL DEFAULT '',
    last_year INTEGER,
    last_season INTEGER,
    last_episode INTEGER,
    last_media_type TEXT NOT NULL DEFAULT 'Movie',
    last_tmdb_id INTEGER,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_session_state_updated ON session_state(updated_at);
