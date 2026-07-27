# Task 1: Fix Hardcoded Scan Directory

## Status: DONE

## Problem

`web/svelte/src/routes/+layout.svelte:22` hardcodes `'/home/morten'` as the
default scan directory prompt. This breaks on any other machine.

## Fix

Replace the hardcoded path with an empty string, or fetch the default from
`GET /api/config` and use `config.scan_directory`.

### Approach: Empty string (simple)

```svelte
const defaultDir = '';
```

User gets an empty prompt and types the path. No network call needed.

### Approach: Config fetch (better UX)

Fetch `/api/config` on mount, use `config.scan_directory || ''` as default.
Adds one API call but gives the user their configured directory pre-filled.

## Files

| File | Change |
|------|--------|
| `web/svelte/src/routes/+layout.svelte` | Line 22: replace hardcoded path |

## Verification

- `cd web/svelte && npm run build`
- `cargo build --all`
- Start server, click Scan — prompt should show empty or configured directory
