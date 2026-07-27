# Task 7: Fix cancel_rip CLI No-Op

## Status: DONE

## Problem

`cli/src/commands/rip.rs:119-123` — the `cancel_rip()` function just
prints "press Ctrl+C" instead of actually cancelling. Since CLI rips
run in the foreground, there's nothing to cancel remotely.

The `Cancel` variant in `RipCommand` is misleading — it suggests
cancellation is possible when it isn't.

## Fix

Remove the `Cancel` variant entirely. CLI rips are foreground processes;
Ctrl+C is the cancellation mechanism.

### Changes

1. Remove `Cancel { index }` from `RipCommand` enum (line 19-22)
2. Remove the match arm in `rip_command()` (line 35)
3. Remove `cancel_rip()` function (lines 119-123)
4. Update help text / doc comments if any reference cancel

## Files

| File | Change |
|------|--------|
| `cli/src/commands/rip.rs` | Remove Cancel variant and handler |

## Verification

- `cargo build --all`
- `cargo run -p cli -- rip --help` — should not show cancel subcommand
- `cargo test --all`
