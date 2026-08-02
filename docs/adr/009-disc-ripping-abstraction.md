# ADR-009: Disc Ripping Backend Abstraction

## Status

Accepted

## Context

Riply supports disc ripping (Blu-ray/DVD) via MakeMKV. The ripping functionality must:
- Support MakeMKV's command-line tool (`makemkvcon`)
- Handle MakeMKV's session-based workflow (open drive → find streams → convert → close drive)
- Support both native MakeMKV installation and Flatpak deployment
- Allow testing without physical drives
- Be replaceable (future: support other ripping tools)

## Decision

Use a **trait-based backend abstraction** for disc ripping.

Key design:
- `RipBackend` trait defines the interface (`open_drive`, `find_streams`, `convert`, etc.)
- `MakemkvBackend` implements the trait for native MakeMKV
- `DryRunBackend` implements the trait for testing without a drive
- Auto-detection of MakeMKV: native install, common paths, Flatpak
- `RipSession` wraps the session lifecycle with `Pin<Box<dyn Future>>` for async trait object compatibility

## Consequences

### Positive
- Easy to add new ripping backends (e.g., HandBrake, dvdbackup)
- `DryRunBackend` enables full integration testing without hardware
- MakeMKV implementation details are isolated in `infra/src/makemkv/`
- Flatpak support transparent to the rest of the codebase

### Negative
- Trait objects require dynamic dispatch (minor performance cost)
- `Pin<Box<dyn Future>>` in traits is complex (documented in AGENTS.md as a known pattern)
- MakeMKV is proprietary software — must be installed separately by the user

### Future Considerations
The trait abstraction makes it straightforward to add:
- HandBrake backend (different workflow, open source)
- dvdbackup backend (DVD-only, simpler)
- Network drive support (remote ripping)
