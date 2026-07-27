# Task 8: Review Page — Add Metadata Editing

## Status: DONE

## Problem

The review page only has Accept/Reject buttons. Users can't edit
canonical_title, canonical_year, series_name, season/episode numbers
before accepting. The backend API (`POST /api/files/:id`) already
supports these updates.

## Fix

Add inline editing fields to each review card:

### UI additions per card

```svelte
<div class="review-edit">
  <input bind:value={file.canonical_title} placeholder="Title" />
  <input bind:value={file.canonical_year} placeholder="Year" type="number" />
  {#if file.media_type === 'Tv'}
    <input bind:value={file.series_name} placeholder="Series" />
    <input bind:value={file.season_number} placeholder="Season" type="number" />
    <input bind:value={file.episode_number} placeholder="Episode" type="number" />
  {/if}
</div>
```

### Accept flow

1. Collect edited fields from the form
2. Call `filesApi.update(id, { canonical_title, canonical_year, ... })`
3. Then call `reviewApi.accept(id)`
4. Remove from queue on success

### Files API

Already has `filesApi.update(id, data)` in `web/svelte/src/lib/api/files.ts`.
Just need to import it in the review page.

## Files

| File | Change |
|------|--------|
| `web/svelte/src/routes/review/+page.svelte` | Add edit form, update accept handler |
| `web/svelte/src/lib/types.ts` | No change needed (ReviewItem type is fine) |

## Verification

- `cd web/svelte && npm run build`
- Start server, go to /review
- Edit title, click Accept — verify file updated in database
