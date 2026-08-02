# ADR-013: Progress Tracking System

## Status

Accepted

## Context

Riply is developed in sessions that may be interrupted, compacted, or lost. When an agent session resumes, it needs to quickly understand:
- What has been completed
- What is in progress
- What should be done next
- How tasks depend on each other

Options considered:
1. **GitHub Issues** — Standard issue tracking, separate from code
2. **File-based task board** — `todo.md` + plan files in the repo
3. **Project management tool** — Linear, Jira, Notion — external to repo
4. **Git commits only** — No explicit task tracking, rely on commit history

## Decision

Use a **file-based system** with two components:

### 1. Task Board (`todo.md`)
- Flat table, one row per task
- Columns: ID, Task, Status, Depends On, Plan File
- Status values: `PENDING`, `IN_PROGRESS`, `DONE`
- ~30 lines max — readable in 5 seconds
- "Next Up" callout at the top highlighting the single next actionable item
- Completed tasks summarized in a collapsible section

### 2. Plan Files (`.opencode/plans/<id>-<name>.md`)
- Detailed implementation specs created **only when work starts**
- Contains: problem statement, required changes, edge cases
- Referenced by `todo.md` via filename

### Why Not GitHub Issues?
- Agents need no auth token or API calls to read it
- Version-controlled alongside code changes
- Survives offline work
- No rate limits or session timeouts
- Direct links to source files in the repo

### Why Not External PM Tools?
- Disconnected from the codebase
- Requires context switching and authentication
- Not version-controlled with code changes
- Fragile when sessions are reconnected

## Task Lifecycle

1. **PENDING** — Task identified, no work started
2. **IN_PROGRESS** — Agent started working on it
3. **DONE** — Implementation complete, tests pass, committed

When starting a task:
1. Read `todo.md` to find the "Next Up" item
2. Open the linked plan file for implementation details
3. Update status to `IN_PROGRESS`
4. When done, update status to `DONE` and add completion note

## Consequences

### Positive
- Agents always know what to do next
- Context survives session compaction
- No external dependencies or authentication
- Task history is in git — diffable, searchable
- Plan files provide implementation context without bloating the task board

### Negative
- No visual Kanban board (but the table is clear)
- Manual status updates required (no automation)
- Plan files can become stale if work is abandoned

### Future Considerations
If the project grows to 50+ concurrent contributors, consider migrating to GitHub Issues. For a single-developer + agent workflow, file-based tracking is optimal.
