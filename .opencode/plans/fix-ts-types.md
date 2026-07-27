# Task 2: Fix Stale TypeScript Types

## Status: DONE

## Problem

`web/svelte/src/lib/types.ts` has type definitions that don't match the
actual API responses from the Rust backend.

### Mismatches

1. **`Job` interface** — has `type`, `created_at`, `completed_at`, `progress`
   (single number). API returns `kind`, `status`, `progress_total`,
   `progress_processed` (no timestamps).

2. **`PipelineStatus`** — includes `'Error'` but Rust enum has `Failed`.

3. **`InstallationStatus`** — missing `flatpak_permissions_ok` field.

4. **`files/+page.svelte`** — filter dropdown uses `"Error"` (line 94),
   statusColor map uses `Error` key (line 62). Should be `"Failed"`.

## Fix

### `types.ts`

```typescript
// PipelineStatus: replace 'Error' with 'Failed'
export type PipelineStatus =
  | 'Scanned'
  | 'Matched'
  | 'NeedsReview'
  | 'Confirmed'
  | 'Moved'
  | 'Failed';

// Job: match API response
export interface Job {
  id: string;
  kind: string;
  status: string;
  progress_total: number;
  progress_processed: number;
}

// InstallationStatus: add flatpak_permissions_ok
export interface InstallationStatus {
  installed: boolean;
  method: DetectionMethod;
  flatpak_available: boolean;
  flathub_configured: boolean;
  flatpak_permissions_ok: boolean;
  drive_access_ok: boolean;
  missing: MissingComponent[];
}
```

### `files/+page.svelte`

- Line 62: `Error: 'error'` → `Failed: 'error'`
- Line 94: `value="Error"` → `value="Failed"`

### `jobs/+page.svelte`

Verify it uses `job.kind`, `job.status`, `job.progress_total`,
`job.progress_processed` (it should already, but confirm).

## Files

| File | Change |
|------|--------|
| `web/svelte/src/lib/types.ts` | Fix Job, PipelineStatus, InstallationStatus |
| `web/svelte/src/routes/files/+page.svelte` | Error → Failed in filter + color map |

## Verification

- `cd web/svelte && npm run build`
- `cargo build --all`
- Check files page filter shows "Failed" and matches database values
