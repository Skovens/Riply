# Remove or Wire Up PipelineTransition

## Status: PENDING

## Problem

`PipelineTransition` in `core/src/pipeline.rs:66-122` defines valid state
transitions for the pipeline but is never used outside this file. It has
tests (lines 179-198) and well-designed methods (`from_state()`, `to_state()`,
`is_valid_from()`), making it potentially useful.

## Options

### Option A: Wire It Up (Recommended)

Use `PipelineTransition` to validate state transitions in the web API.
Currently, `accept_file` and `reject_file` in `web/src/api.rs` do manual
status checks. They could use `PipelineTransition` instead:

```rust
// Current (manual check)
if file.status != PipelineStatus::NeedsReview {
    return Err((StatusCode::BAD_REQUEST, "File not in review state".into()));
}

// With PipelineTransition
let transition = PipelineTransition::ReviewToConfirm;
if !transition.is_valid_from(&file.status) {
    return Err((StatusCode::BAD_REQUEST, "Invalid transition".into()));
}
```

Files to modify:
- `web/src/api.rs` — Use `PipelineTransition` for validation in `accept_file`, `reject_file`
- `app/src/jobs.rs` — Optionally validate transitions when updating file status

### Option B: Remove It

If the team prefers simpler code, remove the dead enum and its tests.
The manual status checks in `web/src/api.rs` are clear enough.

Files to modify:
- `core/src/pipeline.rs` — Remove `PipelineTransition` enum and impl (lines 62-122)
- `core/src/pipeline.rs` — Remove tests (lines 179-198)

## Verification

After either option:
1. `cargo fmt && cargo clippy --all && cargo test --all`
2. If wiring up: Verify `accept_file` and `reject_file` still pass web API tests
3. If removing: Verify all 7 pipeline tests still pass (remaining ones)
