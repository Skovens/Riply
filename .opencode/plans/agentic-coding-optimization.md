# Agentic Coding Optimization for Riply

## Goal

Maximize coding efficiency when working with opencode (or any agentic coding tool).
Minimize context compaction impact. Reduce the chance of an agent making mistakes
due to missing context.

---

## Part 1: How Agentic Coding Works — Key Principles

### The Compaction Problem

Opencode (like all LLM-based tools) has a finite context window. When conversations
grow too long, the system compacts/summarizes earlier messages, losing detail.
The Reddit user's strategy is sound: **break work into small, self-contained tasks
and start a new session for each.**

### Why This Repo is Already Well-Positioned

1. **Strict crate boundaries** — An agent only needs to understand 1-2 crates per task,
   not the whole codebase. The dependency graph enforces this.
2. **AGENTS.md** — Captures build commands, conventions, gotchas, and error patterns.
   Every new session starts with this as context.
3. **Agent skills in `.agents/skills/`** — Domain-specific knowledge (MakeMKV parsing,
   SQLx patterns, TMDb/OMDb APIs) is pre-captured so agents don't need to reverse-engineer it.
4. **Plan files in `.opencode/plans/`** — Records what was done, why, and what's next.
5. **139 tests passing** — Agents can verify their changes immediately.

### The Agent Workflow (Recommended)

```
1. Load AGENTS.md (always provided as context)
2. Load relevant .agents/skills/ files (domain knowledge)
3. Read .opencode/plans/ to understand recent work
4. Execute ONE small task (e.g., "add unit test for X")
5. Run: cargo fmt && cargo clippy --all && cargo test --all
6. Update the relevant plan file with what was done
7. Commit with conventional commit message
8. Start a new session for the next task
```

### Using Subagents Effectively

Subagents are ideal for **parallel exploration and independent tasks**:

| Use subagents for | Use main agent for |
|-------------------|-------------------|
| Reading multiple files to understand a module | Making code changes |
| Searching for patterns across the codebase | Running build/test commands |
| Gathering information for a decision | Writing documentation |
| Checking test coverage in a crate | Implementing a feature |

Example: Before implementing a feature, launch subagents to:
- "Find all consumers of the Database struct"
- "List all unwrap() calls in production code"
- "Check test coverage for app/src/mover.rs"

Then use the main agent to implement based on the findings.

---

## Part 2: Current Gaps (Action Items)

### HIGH PRIORITY

#### 1. Fix the SvelteKit Skill (Wrong Technology)

**File:** `.agents/skills/sveltekit-realtime.md`
**Problem:** Documents SvelteKit + Tailwind CSS, but the frontend is vanilla JS
with ES modules and custom CSS. An agent following this skill would generate
wrong code (Svelte components, Tailwind classes, Svelte stores).
**Fix:** Rewrite as `vanilla-js-frontend.md` documenting the actual architecture:
- ES modules (no bundler)
- Hash-based routing (`#/dashboard`, `#/files`, etc.)
- `RiplyApp` class in `app.js`
- `store.js` for state management (simple pub/sub)
- Component pattern: `renderDashboard()`, `renderFiles()`, etc.
- API client in `api.js`
- CSS custom properties for theming (dark/light)

#### 2. Write a README.md

**File:** `README.md` (currently 2 lines)
**Problem:** New agents have no quick onboarding. The README is the first thing
anyone sees.
**Fix:** Add:
- Project description and purpose
- Feature list (scan, analyze, review, move, rip)
- Architecture diagram (the crate dependency graph)
- Quick start (clone, build, run)
- Link to AGENTS.md for agent-specific instructions

#### 3. Add Per-Crate READMEs

**Files to create:**
- `core/README.md` — Domain types, traits, business rules
- `app/README.md` — Services, job scheduler, orchestration
- `infra/README.md` — I/O adapters, database, HTTP, MakeMKV
- `web/README.md` — API endpoints, SSE, frontend architecture
- `cli/README.md` — CLI commands, entry point

Each should be 10-20 lines describing:
- What this crate does
- Key public types
- How to test it
- Common patterns to follow

This is the **single highest-impact change** for agent efficiency. An agent
working on `app/src/mover.rs` only needs to read `app/README.md` to understand
the crate's purpose, instead of reading every file to infer it.

#### 4. Document the Frontend Architecture

**File to create:** `web/README.md` or `docs/frontend-architecture.md`
**Problem:** The frontend uses a custom vanilla JS pattern that's undocumented.
An agent wouldn't know:
- How routing works (`RiplyApp.navigate()`)
- How state management works (`store.js` pub/sub)
- How components are structured (render functions, not classes)
- How SSE integration works (`connectSSE()`, polling fallbacks)
- The API client pattern (`api.js` methods)

---

### MEDIUM PRIORITY

#### 5. Complete the TV Metadata Plan

**File:** `.opencode/plans/tv-metadata.md`
**Problem:** Outlines 5 phases but has no status, no implementation record.
If the agent session that implements this gets compacted, the context is lost.
**Fix:** Either:
- Mark as "IN PROGRESS" and implement the phases, or
- Mark as "PENDING" with a clear description of what needs to be done

#### 6. Add CLI Integration Tests

**Problem:** `cli/` has ~600 lines of command logic with zero tests.
The workspace already has `assert_cmd` and `predicates` as dev-dependencies.
**Fix:** Create `cli/tests/` with integration tests for:
- Argument parsing (clap derive)
- Command output verification
- Error message content

#### 7. Add Unit Tests for `web/src/api.rs`

**Problem:** 963 lines of HTTP handlers with only integration tests.
No tests for error paths (e.g., `accept_file` with wrong status).
**Fix:** Add unit tests for:
- `accept_file` / `reject_file` validation
- `PaginatedResponse` edge cases
- `message_to_event` in SSE

#### 8. Remove Dead Code: `PipelineTransition`

**File:** `core/src/pipeline.rs:66`
**Problem:** `PipelineTransition` enum is defined but never used anywhere.
Agents might try to use it or get confused by its presence.
**Fix:** Remove it, or if it's intended for future use, add a `// TODO: wire up`
comment and document the intended usage.

#### 9. Add `Database` Trait Abstraction

**Problem:** `Database` is a concrete struct in `infra`, not a trait in `core`.
This means:
- Cannot mock the database without in-memory SQLite
- Web and CLI both depend on `infra::Database` concretely
- Harder to swap backends

**Fix:** Define a `Database` trait in `core` or `infra` with methods:
```rust
pub trait Database: Send + Sync {
    async fn save_file(&self, record: &FileRecord) -> Result<(), DbError>;
    async fn get_file(&self, id: &str) -> Result<Option<FileRecord>, DbError>;
    async fn get_files_by_status(&self, status: &str, page: usize, per_page: usize)
        -> Result<(Vec<FileRecord>, usize), DbError>;
    // ... etc
}
```

Then implement it for `SqliteDatabase` and `MockDatabase`.

---

### LOW PRIORITY

#### 10. Add `anyhow::Context` Usage

**Problem:** No `.context("...")` calls anywhere in the codebase.
Error messages lack context about where failures occurred.
**Fix:** Add context at key boundaries:
- "Failed to connect to TMDb API"
- "Database migration failed"
- "Failed to scan directory: {path}"

#### 11. Implement CLI `cancel_rip`

**Problem:** `cancel_rip` in CLI is a no-op stub. The web API has proper
cancellation, but the CLI just prints a message.
**Fix:** Either implement it (query active jobs and cancel) or remove the
subcommand.

#### 12. Add More ADRs

**Suggested ADRs:**
- ADR-002: Metadata search strategy (TMDb + OMDb fallback)
- ADR-003: Ripping backend abstraction (trait-based design)
- ADR-004: SSE-based real-time updates
- ADR-005: Job scheduler design

---

## Part 3: Recommended Task Breakdown Strategy

### What Makes a Good "Agent Task"

A good task for agentic coding is:
1. **Small** — Can be completed in one session (under compaction threshold)
2. **Self-contained** — Doesn't require understanding the entire codebase
3. **Verifiable** — Has clear success criteria (tests pass, builds)
4. **Scoped** — Confined to 1-2 files or 1 crate

### Examples of Good Tasks

| Good Task | Why It Works |
|-----------|-------------|
| "Add unit tests for `parse_filename` in `analyzer.rs`" | Single file, clear input/output, existing test patterns to follow |
| "Fix the sveltekit-realtime.md skill to match the actual vanilla JS frontend" | Self-contained, no code changes needed |
| "Add Database trait definition in infra" | Well-scoped, follow existing trait patterns |
| "Remove unused PipelineTransition enum" | One file, one change, tests verify nothing breaks |
| "Add CLI integration tests for scan command" | Uses existing assert_cmd setup, clear output to verify |

### Examples of Bad Tasks (Too Large)

| Bad Task | Why It Fails |
|----------|-------------|
| "Implement TV metadata enrichment" | 5 phases, touches core/app/infra, needs API knowledge |
| "Redesign the web UI" | Many files, no clear boundaries, subjective decisions |
| "Add PostgreSQL support" | Changes Database trait, migrations, connection pooling |
| "Improve error handling" | Vague, touches every file, no clear success criteria |

### Breaking Large Tasks into Small Ones

**"Implement TV metadata enrichment"** should be split into:
1. "Add TV fields to MetadataMatch struct in core"
2. "Add SxxExx pattern detection to parse_filename in analyzer.rs"
3. "Add TMDb episode detail fetcher in infra/http.rs"
4. "Wire TV fields into AnalyzerService in app/analyzer.rs"
5. "Add OMDb TV search in infra/http.rs"
6. "Add unit tests for new TV parsing logic"

Each is a self-contained task that can be done in one session.

---

## Part 4: Session Management Best Practices

### Starting a New Session

1. AGENTS.md is always loaded automatically — trust it as the source of truth
2. Check `.opencode/plans/` for recent context
3. If working on a specific feature, check if a plan file exists for it
4. Read the relevant `*/README.md` files for the crates you'll touch

### During a Session

1. Keep tasks small and focused
2. Run `cargo check` frequently (fast feedback)
3. Run `cargo fmt && cargo clippy --all && cargo test --all` before finishing
4. Update plan files when making changes
5. If a task is getting complex, split it and start a new session

### Before Ending a Session

1. Commit with a conventional commit message
2. Update the relevant plan file with:
   - What was done
   - Files modified
   - Any decisions made
   - Remaining work
3. If you discovered new gotchas, add them to AGENTS.md "Known Gotchas" section

### Using Subagents

Launch subagents for:
- "Find all consumers of struct X"
- "Count tests in each crate and list untested modules"
- "Search for all unwrap() calls in production code"
- "List all TODO/FIXME comments"

Use the main agent for:
- Making code changes
- Running build/test commands
- Writing documentation
- Creating new files

---

## Part 5: Quick Reference — What Agents Know vs Need to Learn

### Automatically Known (from AGENTS.md)

- Build commands: `cargo fmt`, `cargo clippy --all`, `cargo build --all`, `cargo test --all`
- Expected results: 0 clippy warnings, 131+ tests passing
- Error handling: `thiserror` in core, `anyhow::Result` in app/infra/cli
- Import convention: `riply_core` not `core`
- Commit format: `feat(scope): description`
- Axum 0.7 route syntax: `:param` not `{param}`
- MakeMKV as replaceable backend
- `Pin<Box<dyn Future>>` for async traits
- Database auto-creation with WAL mode
- `web` is library-only, binary is in `cli`

### Needs Documentation (Gaps)

- Frontend architecture (vanilla JS, routing, state, components)
- Per-crate purpose and key types (no crate-level READMEs)
- API endpoint reference (what endpoints exist, what they return)
- Job scheduler lifecycle (how jobs are created, tracked, cancelled)
- SSE event format and consumer patterns
- Test patterns specific to this project (TestContext, MockSearcher, wiremock setup)

---

## Part 6: Implementation Priority

If you want to maximize agent efficiency with minimal effort, do these first:

1. **Fix `sveltekit-realtime.md`** — Prevents agents from generating wrong code. 30 min.
2. **Write `README.md`** — Quick onboarding for any new agent. 15 min.
3. **Add `core/README.md` and `app/README.md`** — The two crates agents touch most. 30 min.
4. **Document frontend architecture** — Prevents agents from guessing the pattern. 20 min.
5. **Remove `PipelineTransition` dead code** — One line edit. 1 min.

After these 5 changes, agents will be significantly more efficient with less
context loss and fewer mistakes.
