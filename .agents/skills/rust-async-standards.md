---
name: rust-async-standards
description: >
  Best practices for Rust development in the Riply project.
  Covers Tokio async runtime, Cargo workspace management, error handling,
  serialization, and CLI development.
metadata:
  language: rust
  version: "1.75+"
  compatibility: rust, cargo, tokio, clap, thiserror, serde, proptest
---

# Rust Async Standards

## General Principles
- **Idiomatic Rust:** Follow the Rust API Guidelines.
- **Safety:** Prefer safe Rust. Use `unsafe` only when necessary and well-documented.
- **Performance:** Profile before optimizing. Use `criterion` for benchmarks.

## Async Runtime (`tokio`)
- Use `tokio::main` for the entry point.
- Prefer `tokio::spawn` for fire-and-forget tasks.
- Use `tokio::join!` for concurrent operations where results are needed immediately.
- Use `tokio::select!` for multiplexing multiple async streams.
- Avoid blocking calls in async contexts. Use `tokio::task::spawn_blocking` for I/O or CPU-intensive tasks.

## Error Handling
- Use `thiserror` for library errors. Implement `Display` and `Error`.
- Use `anyhow` for application-level errors (e.g., in `main` or CLI commands).
- Propagate errors using the `?` operator.
- Do not panic unless it's a bug that cannot be recovered from.

## CLI Development (`clap`)
- Use `clap` with `derive` for argument parsing.
- Structure commands as subcommands (e.g., `scan`, `analyze`, `move`).
- Provide clear help text and examples.

## Serialization (`serde`)
- Use `serde` with `derive` for structs that need serialization/deserialization.
- Prefer `serde_json` for JSON.
- Use `toml` for configuration files.

## Testing
- Use `proptest` for property-based testing (critical for confidence/filename logic).
- Use `wiremock` for mocking HTTP requests.
- Use `assert_cmd` and `predicates` for CLI testing.
- Aim for high coverage on core logic.