# ADR-027: Tmp Folder Lifecycle

## Status

Proposed

## Context

Ripped content is written to a configurable tmp folder (default `/tmp/riply`). This folder has two purposes:
1. **Temporary staging** — Ripped content lives here until named and moved to the library
2. **Safety net** — If ripping fails or naming is uncertain, content remains accessible

The question is how long content should stay in the tmp folder, and when (if ever) it should be cleaned up.

Options considered:
1. **No cleanup** — Content stays forever until user moves it. Simple but leads to disk bloat.
2. **Immediate cleanup** — Content deleted after rip completes. Safe but risky if rip fails mid-way.
3. **Age-based cleanup** — Content older than N days deleted on server restart. Balanced approach.
4. **Manual cleanup** — User explicitly deletes content via UI. Adds complexity.

## Decision

Use **age-based cleanup on server restart**.

### Behavior

| Scenario | Action |
|----------|--------|
| Rip completes successfully | Content moved to library destination |
| Rip completes, unidentified | Content stays in tmp with disc name |
| Rip fails | Partial content stays in tmp |
| Server restart | Content older than N days deleted |
| Content < N days old | Kept, regardless of state |

### Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `ripping.tmp_dir` | `/tmp/riply` | Tmp folder path |
| `ripping.tmp_cleanup_age_days` | `7` | Days before cleanup |

### Implementation

**On server startup:**
```rust
fn cleanup_tmp_folder(tmp_dir: &Path, max_age_days: u64) {
    let cutoff = SystemTime::now() - Duration::days(max_age_days as i64);
    for entry in tmp_dir.read_dir() {
        let folder = entry.unwrap();
        let metadata = folder.metadata().unwrap();
        if let Ok(modified) = metadata.modified() {
            if modified < cutoff {
                fs::remove_dir_all(folder.path());
            }
        }
    }
}
```

**On server shutdown:**
- No cleanup — let startup handle it
- This prevents deleting content that was just ripped but hasn't been handled yet

### UI Implications

- Drive card shows "Unidentified — view in Organizer" for content in tmp
- Organizer window can scan the tmp folder as a scan directory
- Settings shows tmp folder path and cleanup age

### Rationale

**Why not immediate cleanup?**
- If a rip fails mid-way (disc scratch, MakeMKV error), partial content is lost
- Users need time to handle unidentified rips

**Why not no cleanup?**
- `/tmp/riply` can grow indefinitely
- Users don't expect the app to manage their disk space

**Why not manual cleanup?**
- Adds UI complexity
- Users forget to clean up
- Age-based is automatic and predictable

**Why on restart?**
- Server restart is a natural cleanup point
- Prevents deleting content that was just ripped
- No background cleanup process needed

### Consequences

**Positive:**
- Automatic cleanup — no user action needed
- Safe — fresh rips are preserved
- Simple — no cleanup UI needed
- Predictable — content stays for N days, then gone

**Negative:**
- Content older than N days is deleted without warning
- Users who forget to handle rips lose their content
- No way to recover deleted content

### Mitigation

- Document the cleanup policy in Settings (show countdown: "Content older than 7 days is auto-deleted on restart")
- Organizer window can scan tmp folder — users can review and move content before it's deleted
- Users can change `tmp_cleanup_age_days` in Settings if they want longer retention

## Supersedes

- ADR-023 (Per-Drive Autorip State Is Session-Scoped) — tmp folder cleanup is related but separate
