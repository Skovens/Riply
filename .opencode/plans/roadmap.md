# Roadmap

## Progress

| # | Item | Status |
|---|------|--------|
| 1 | Name cleaning module (`core/src/name_cleaner.rs`) | **In progress** |
| 2 | Frontend tests (Vitest + Svelte testing) | Pending |
| 3 | Rip-time metadata search | Pending |
| 4 | Rate limiting for metadata APIs | Pending |
| 5 | Folder-based scanning + TitleGroup | Pending |
| 6 | TV show extras / Jellyfin layout | Pending |

---

## Item 1: Name Cleaning Module

**Goal:** Create `core/src/name_cleaner.rs` with `clean_name()` (title-case + unicode→ASCII), refactor `parse_disc_name` to use it.

**Commits:**
- (none yet)

**Decisions:**
- `clean_name` = unidecode + Inflector title-case + collapse whitespace. No regex in core.
- `parse_disc_name` delegates title-casing to `clean_name`, keeps own year-extraction logic.
- `parse_filename` left untouched — it doesn't title-case and uses regex patterns that belong in `app`.

**Gotchas:**
- (none yet)

---

## Up Next

### Item 2: Frontend Tests
- Install vitest + @testing-library/svelte
- Write API client tests + one component test

### Item 3: Rip-Time Metadata Search
- Inject MetadataSearcher into CLI/web ripper
- Search TMDb using cleaned disc name, name output directory with result

### Item 4: Rate Limiting
- governor crate, per-provider RateLimiter trait

### Item 5: Folder-Based Scanning
- TitleGroup entity, folder-first scan, DB migration

### Item 6: TV Extras
- extras/ directory detection, mover extras handling, UI
