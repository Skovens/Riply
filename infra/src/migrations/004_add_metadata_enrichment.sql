-- Add metadata enrichment columns to the files table.
-- Genres and cast are stored as JSON-encoded TEXT arrays; the adapter
-- serializes/deserializes them. Column is named cast_list (not cast) because
-- CAST is an SQLite keyword and cannot be used as an unquoted column name.
ALTER TABLE files ADD COLUMN genres TEXT;
ALTER TABLE files ADD COLUMN runtime INTEGER;
ALTER TABLE files ADD COLUMN content_rating TEXT;
ALTER TABLE files ADD COLUMN overview TEXT;
ALTER TABLE files ADD COLUMN cast_list TEXT;
