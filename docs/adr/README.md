# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for Riply. An ADR captures a significant architectural decision — a context, the decision itself, and the consequences that follow.

## ADR Status

| Status | Description |
|--------|-------------|
| **Proposed** | A decision being considered, not yet accepted |
| **Accepted** | The decision has been approved and is in effect |
| **Deprecated** | The decision is no longer recommended for new work |
| **Superseded** | A newer ADR has replaced this decision |

## Review Process

- **Authors** propose ADRs with status **Proposed**
- **Maintainers** review and discuss Proposed ADRs (at least one approval required)
- Once accepted, status changes to **Accepted**
- If a better alternative emerges, create a new ADR that supersedes the old one
- For minor clarifications (typos, added consequences), update in place without a new number

## ADR Template

When creating a new ADR, copy this template:

```markdown
# ADR-NNN: [Title]

## Status
[Proposed | Accepted | Deprecated | Superseded]

## Context
What is the issue or opportunity being addressed? What background information is needed?

## Decision
What is the decision being made? What are the options considered?

## Consequences
What are the positive and negative consequences of this decision?
What becomes easier or more difficult to do?
```

## Index

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [001](001-workspace-architecture.md) | Workspace Crate Architecture | Accepted | 2026-07-26 |
| [002](002-database-technology.md) | Database Technology Choice | Accepted | 2026-08-02 |
| [003](003-frontend-approach.md) | Frontend Approach | Accepted | 2026-08-02 |
| [004](004-error-handling.md) | Error Handling Strategy | Accepted | 2026-08-02 |
| [005](005-async-runtime.md) | Async Runtime Selection | Accepted | 2026-08-02 |
| [006](006-http-framework.md) | HTTP Framework Selection | Accepted | 2026-08-02 |
| [007](007-cli-framework.md) | CLI Framework Selection | Accepted | 2026-08-02 |
| [008](008-logging-observability.md) | Logging and Observability | Accepted | 2026-08-02 |
| [009](009-disc-ripping-abstraction.md) | Disc Ripping Backend Abstraction | Accepted | 2026-08-02 |
| [010](010-job-scheduler.md) | Background Job System | Accepted | 2026-08-02 |
| [011](011-real-time-updates.md) | Real-Time Web Updates | Accepted | 2026-08-02 |
| [012](012-testing-strategy.md) | Testing Strategy | Accepted | 2026-08-02 |
| [013](013-progress-tracking.md) | Progress Tracking System | Accepted | 2026-08-02 |
| [014](014-router-splitting.md) | Router Splitting (Per-Domain Modules) | Accepted | 2026-08-02 |
| [015](015-typed-web-error.md) | Typed Web Error Type | Accepted | 2026-08-02 |
| [016](016-sqlx-migration-system.md) | SQLx Migration System | Accepted | 2026-08-02 |
| [017](017-api-versioning.md) | API Versioning | Accepted | 2026-08-02 |
| [018](018-graceful-shutdown.md) | Graceful Shutdown | Accepted | 2026-08-02 |
| [019](019-xdg-data-home.md) | XDG Data Home for Config and Database | Proposed | 2026-08-12 |
| [020](020-rip-progress-bar.md) | Rip Progress Bar in Drive Panel | Proposed | 2026-08-12 |
| [021](021-collapsible-folder-preview.md) | Collapsible Folder Preview in the Files Pane | Proposed | 2026-08-12 |
| [022](022-single-media-naming-service.md) | Single Media-Naming Service | Proposed | 2026-08-12 |

## Creating a New ADR

1. Copy the template above to `docs/adr/NNN-title.md`
2. Use the next sequential number
3. Set status to **Proposed** initially
4. Discuss and update to **Accepted** when agreed upon
5. Update this index
6. Reference the ADR in relevant code comments and documentation

## Documentation Note

`docs/frontend-architecture.md` contains detailed frontend system design documentation.
It is not an ADR — it describes *how* the frontend works rather than *why* a decision was made.
Keep it in `docs/` alongside ADRs for discoverability, but do not number it as an ADR.
