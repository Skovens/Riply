# ADR-012: Testing Strategy

## Status

Accepted

## Context

Riply needs comprehensive testing across its layers:
- **Domain logic** (core): File hashing, media type detection, metadata parsing
- **Service logic** (app): Scan workflows, analysis pipelines, move operations
- **Integration** (infra): Database queries, HTTP client calls, subprocess execution
- **End-to-end** (cli): Full command workflows

The testing strategy must:
- Be fast (CI should complete in minutes)
- Require no external dependencies (no test databases, no mock servers in CI)
- Support both unit and integration testing
- Handle hardware-dependent code (MakeMKV) without physical drives

## Decision

Use a **multi-layered testing strategy** with four techniques:

### 1. Unit Tests — Inline `#[cfg(test)]` modules
- Tests co-located with source code
- Fast, focused, no setup required
- Example: `core/src/file_record.rs` contains tests for file validation

### 2. Property-Based Testing — `proptest` crate
- Generate thousands of random inputs to find edge cases
- Used for data validation (file records, media types)
- Example: `proptest` for `FileRecord` parsing and validation

### 3. HTTP Mocking — `wiremock` crate
- Mock external API responses (TMDb, OMDb)
- Deterministic tests without network calls
- Example: `infra` crate tests TMDb metadata lookup with fixed responses

### 4. Dry-Run Modes — `DryRunBackend`
- Simulate MakeMKV operations without a physical drive
- Full integration testing of ripping workflow
- Example: Test session lifecycle, stream detection, conversion

### 5. Integration Tests — `cli/tests/` directory
- End-to-end CLI testing with `assert_cmd` and `predicates`
- Test command argument parsing and output
- Example: Verify `riply status` outputs expected format

## Consequences

### Positive
- No external test infrastructure required
- Property-based testing finds edge cases manual tests miss
- Dry-run modes enable full workflow testing without hardware
- Wiremock makes external API tests deterministic
- 130+ tests provide regression protection

### Negative
- Property-based testing has a learning curve
- Dry-run backends may not cover all edge cases of real hardware
- No UI/frontend tests (vanilla JS is hard to test without a browser)
- Database tests use in-memory SQLite (may not catch disk-specific issues)

### Future Considerations
Consider adding:
- Snapshot testing for CLI output (detect unintended changes)
- Load testing for the web server (concurrent requests)
- E2E browser tests with Playwright/Cypress (if frontend grows significantly)
