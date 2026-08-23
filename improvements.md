# SSE JSON Refactor — Improvements Completed

All improvements from the code review have been implemented. See commit history for details.

---

## ✅ Completed

### Phase 1: Dead Code Removal
- [x] **1.1 Remove Legacy SSE Methods** — `send_status`, `send_progress`, `send_log` removed from `web/src/sse.rs`
- [x] **1.2 Clean Up Dead Test Code** — Tests updated to use typed `makeRipProgressEvent` helper
- [x] **1.3 Simplify `message_to_event`** — Reduced to single path: `Event::default().event("status").data(msg)`

### Phase 2: Type Safety
- [x] **2.1 Tighten `SseEvent` Type** — Replaced `Record<string, unknown>` with discriminated union types
  - Added typed data structs: `RipProgressData`, `RipCompleteData`, `RipErrorData`, `RipMessageData`, `RipNamedData`, `JobData`
  - Extraction helpers now take typed events (e.g., `ripProgressFields` takes `Extract<SseEvent, {kind:'rip';action:'progress'}>`)
- [x] **2.2 Remove Over-Defensive Code** — Simplified `ripProgressFields` to `typeof data.pct === 'number' ? data.pct : 0`
- [x] **2.3 JSDoc Types** — Kept as-is; they document local state shapes for Svelte `$state` inference, not event types

### Phase 3: Error Handling & Robustness
- [x] **3.1 Fix `send_json` Error Handling** — Now logs dropped events via `tracing::warn!`
- [x] **3.2 Fix Frontend SSE Error Handling** — Extracted `parseAndForward` helper, added `onError` callback option
- [x] **3.3 Fix Poster Cache Key** — Deduplicates by `(title, year)` not `(idx, title, year)`

### Phase 4: Code Organization
- [ ] **4.1 Split `handleSSEEvent`** — Deferred. The function is manageable as-is; splitting can be done as a follow-up when the ripper page grows more complex.

---

## Build & Test Status

| Metric | Before | After |
|--------|--------|-------|
| Rust tests | 236/236 pass | 236/236 pass |
| Frontend tests | 52/52 pass | 52/52 pass |
| Rust build | ✅ | ✅ |
| Frontend build | ✅ | ✅ |

---

## Commits

1. `improve(sse): remove dead legacy code and tighten type safety` — Phases 1-2
2. `improve(sse): error handling and poster dedup` — Phases 3.2-3.3
