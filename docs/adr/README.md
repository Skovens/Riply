# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for Riply. An ADR captures a significant architectural decision — a context, the decision itself, and the consequences that follow.

## ADR Status

| Status | Description |
|--------|-------------|
| **Proposed** | A decision being considered, not yet accepted |
| **Accepted** | The decision has been approved and is in effect |
| **Deprecated** | The decision is no longer recommended for new work |
| **Superseded** | A newer ADR has replaced this decision |

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
| [001](001-workspace-architecture.md) | Workspace Crate Architecture | Accepted | 2024-01-XX |
| [002](002-database-technology.md) | Database Technology Choice | Accepted | 2024-01-XX |
| [003](003-frontend-approach.md) | Frontend Approach | Accepted | 2024-01-XX |
| [004](004-error-handling.md) | Error Handling Strategy | Accepted | 2024-01-XX |
| [005](005-async-runtime.md) | Async Runtime Selection | Accepted | 2024-01-XX |
| [006](006-http-framework.md) | HTTP Framework Selection | Accepted | 2024-01-XX |
| [007](007-cli-framework.md) | CLI Framework Selection | Accepted | 2024-01-XX |
| [008](008-logging-observability.md) | Logging and Observability | Accepted | 2024-01-XX |
| [009](009-disc-ripping-abstraction.md) | Disc Ripping Backend Abstraction | Accepted | 2024-01-XX |
| [010](010-job-scheduler.md) | Background Job System | Accepted | 2024-01-XX |
| [011](011-real-time-updates.md) | Real-Time Web Updates | Accepted | 2024-01-XX |
| [012](012-testing-strategy.md) | Testing Strategy | Accepted | 2024-01-XX |

## Creating a New ADR

1. Copy the template above to `docs/adr/NNN-title.md`
2. Use the next sequential number
3. Set status to **Proposed** initially
4. Discuss and update to **Accepted** when agreed upon
5. Update this index
6. Reference the ADR in relevant code comments and documentation
