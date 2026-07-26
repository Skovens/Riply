# riply_core

Domain types, traits, and business rules. Zero infrastructure dependencies.

## Key Types

- `FileRecord` — Media file metadata (path, status, title, year, etc.)
- `MediaType` — Movie or TV episode
- `MetadataMatch` / `MetadataSearcher` — Trait for searching TMDb/OMDb
- `PipelineStatus` — File lifecycle (Scanned → Analyzed → Reviewed → Confirmed → Moved)
- `RipBackend` / `RipSession` / `RipHandle` — Traits for ripping backends
- `DriveInfo` / `DriveState` / `RipEvent` / `RipError` — Ripping domain types

## Testing

```bash
cargo test -p riply_core
```

## Patterns

- Import as `riply_core::` (not `core`) to avoid conflicting with std.
- Use `thiserror` for domain errors.
- `MetadataSearcher` and `RipSession` use `Pin<Box<dyn Future>>` for dyn compatibility.
