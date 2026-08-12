# ADR-021: Collapsible Folder Preview in the Files Pane

## Status

Proposed

## Context

The dashboard files pane lists every file from a rip flat
(`filesApi.list` in `+page.svelte`). A single disc can produce dozens of
files, so the view is noisy and does not reflect structure. Users want
to see the folder/disc, and only expand to files when needed.

The `files.source` column cannot be used for grouping: it records
provenance (the analyzer sets it to `"tmdb"`), not a directory.

## Decision

Group the dashboard files pane by folder (the file's parent directory,
or `title_groups.source_directory` for rip output).

- Collapsed: show only the folder row with a file count.
- Expanded: show the files under it.
- When a show/movie has been correctly identified, each expanded file
  row shows old name -> new name (current name -> canonical target),
  reusing the data already produced by the move preview
  (`/v1/move/preview`, `MovePreviewItem { file, dest_path }`) rather
  than re-deriving naming rules client-side.
- The dedicated `/files` page keeps its flat, paginated, filterable
  list; this ADR targets the dashboard preview/files pane.

## Consequences

- The pane shows structure (which disc, how many files) without dumping
  every file.
- Old -> new names surface the effect of naming decisions exactly where
  the user looks (and pair with the naming modal, ADR-022).
- Requires a grouping layer in the frontend and reusing the move-preview
  payload for the rename display.
- `/files` page behavior is unchanged.
