# Task 5: Regex Lazy Static in Analyzer

## Status: DONE

## Problem

`app/src/analyzer.rs:148-188` compiles 7 regexes from scratch on every
filename analysis call. This is wasteful since the patterns are constant.

## Fix

Use `std::sync::LazyLock<Regex>` (stable since Rust 1.80) to compile
each regex once at first use:

```rust
use std::sync::LazyLock;
use regex::Regex;

static SXXEXX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S(\d{1,2})E(\d{1,3})").unwrap());

static SXX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S(\d{1,2})(?:\s|$)").unwrap());

// ... etc for all 7 patterns
```

### Patterns to extract (from analyzer.rs)

1. `S(\d{1,2})E(\d{1,3})` — SxxExx (line 148)
2. `S(\d{1,2})(?:\s|$)` — Sxx season pack (line 156)
3. `E(\d{1,3})(?:\s|$)` — Exx without season (line 164)
4. `(\d{1,2})x(\d{1,3})` — NxNN format (line 172)
5. `\((\d{4})\)` — Year in parens (line 181)
6. `(\d{4})` — Year bare (line 184)
7. Title cleanup regex (line 188) — check exact pattern

## Files

| File | Change |
|------|--------|
| `app/src/analyzer.rs` | Replace `Regex::new()` calls with `LazyLock` statics |

## Verification

- `cargo test --all` — all analyzer tests must still pass
- `cargo clippy --all` — no warnings
