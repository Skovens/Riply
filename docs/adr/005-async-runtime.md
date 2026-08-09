# ADR-005: Async Runtime Selection

## Status

Accepted

## Context

Riply is I/O-bound: database queries, HTTP requests, file system operations, and subprocess execution. The async runtime must:
- Handle concurrent I/O efficiently
- Integrate well with the Rust ecosystem
- Support Tokio-compatible crates
- Be production-ready and well-maintained

Options considered:
1. **Tokio** — Most popular Rust async runtime, largest ecosystem
2. **async-std** — Standard-library-like API, smaller ecosystem
3. **smol** — Minimalist, event-loop based, good for simple apps

## Decision

Use **Tokio** as the async runtime with `features = ["full"]`.

Rationale:
- Largest ecosystem of async crates are Tokio-native
- `features = ["full"]` includes all needed capabilities (macros, rt-multi-thread, signal, sync, io-util, time)
- Well-documented and battle-tested in production
- Axum, SQLx, and reqwest all build on Tokio

## Consequences

### Positive
- Maximum crate compatibility
- Multi-threaded runtime handles CPU-bound tasks alongside I/O
- Rich feature set (signals, timers, IPC, tracing integration)
- Industry standard — easy to find help and examples

### Negative
- Larger binary size than minimal runtimes (negligible for Riply)
- "Full" features include some unused capabilities (acceptable for simplicity)

### Future Considerations
If binary size becomes critical (e.g., embedded deployment), consider switching to a minimal Tokio feature set or migrating to smol. For Riply's use case, this is unlikely.
