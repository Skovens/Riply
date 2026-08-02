# ADR-003: Frontend Approach

## Status

Accepted

## Context

Riply needs a web dashboard for real-time monitoring and interaction with the media pipeline. The frontend must:
- Be embedded in a single Rust binary (no separate deployment)
- Work without a build step for end users
- Support real-time updates (job progress, SSE events)
- Be maintainable as the feature set grows

Options considered:
1. **Embedded vanilla JS** — Static files served from the Rust binary
2. **SvelteKit** — Full build pipeline, HMR for development
3. **Server-side rendering** — Template-based rendering on each request
4. **Headless API only** — No embedded frontend, external clients only

## Decision

Use **embedded vanilla JS** for production, with an optional **SvelteKit** development server for HMR.

Production:
- Static HTML/CSS/JS files embedded in the binary via `include_str!`
- Hash-based routing (`#/dashboard`, `#/files`, etc.)
- Pub/sub state management via a simple `Store` class
- Component-based rendering with explicit render functions
- Server-Sent Events for real-time updates

Development:
- SvelteKit on port 5173 for hot module replacement
- Build output baked into the binary for production (`npm run build`)

## Consequences

### Positive
- Zero JavaScript toolchain required for end users
- Single binary distribution works everywhere
- Full control over the frontend without framework overhead
- SvelteKit HMR speeds up development
- Easy to embed in any Rust binary

### Negative
- No virtual DOM — manual DOM updates as features grow
- No TypeScript — runtime type errors instead of compile-time checks
- Larger files to maintain as a monolith vs. framework-managed components
- State management is custom, not battle-tested like Redux/Zustand

### Future Considerations
If the frontend grows beyond ~10k lines, consider migrating to a compiled-to-JS framework (Svelte, Yew, Leptos) for type safety and better code organization. The current component pattern makes this transition feasible.
