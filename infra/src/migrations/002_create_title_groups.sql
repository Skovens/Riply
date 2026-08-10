-- Create title_groups table
CREATE TABLE IF NOT EXISTS title_groups (
    id TEXT PRIMARY KEY,
    source_directory TEXT NOT NULL,
    title TEXT NOT NULL,
    year INTEGER,
    tmdb_id INTEGER,
    media_type TEXT NOT NULL DEFAULT 'movie',
    series_name TEXT,
    status TEXT NOT NULL DEFAULT 'New'
);

-- Add title_group_id column to files (no FK constraint; enforced at app level)
ALTER TABLE files ADD COLUMN title_group_id TEXT;

-- Create index for lookups
CREATE INDEX IF NOT EXISTS idx_files_title_group ON files(title_group_id);
CREATE INDEX IF NOT EXISTS idx_title_groups_status ON title_groups(status);
