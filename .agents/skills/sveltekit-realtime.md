---
name: sveltekit-realtime
description: >
  Frontend development standards for Riply using SvelteKit.
  Covers component structure, Tailwind CSS usage, SSE integration for real-time progress,
  and API client patterns.
metadata:
  language: svelte, typescript
  version: "1.0+"
  compatibility: sveltekit, tailwind, sse, axios
---

# SvelteKit Realtime Frontend

## General Principles
- **Atomic Design:** Use atomic design principles (atoms, molecules, organisms).
- **Reactivity:** Leverage Svelte's reactivity for real-time updates.
- **Performance:** Optimize bundle size and lazy-load heavy components.

## Component Structure
- **Atoms:** Basic UI elements (Button, Input, Badge).
- **Molecules:** Combinations of atoms (ProgressBar, DataTable).
- **Organisms:** Complex components (Dashboard, ReviewPanel).
- **Templates:** Page-level layouts (DashboardPage, ReviewPage).

## Tailwind CSS
- Use utility classes for styling.
- Define custom configuration in `tailwind.config.js`:
  - Riply-specific colors (e.g., `status-success`, `status-error`).
  - Spacing scale for consistent layout.
  - Typography styles for logs and data tables.

## SSE Integration
- Use `EventSource` or a custom Svelte store for Server-Sent Events.
- Subscribe to progress channels:
  - `scan_progress`: File scanning progress.
  - `analyze_progress`: Media analysis progress.
  - `mover_progress`: File moving progress.
  - `ripper_status`: Drive/ripper status updates.
- Handle connection errors and reconnection logic.

## API Client
- Use a centralized API client (Axios or fetch wrapper).
- Implement interceptors for:
  - Error handling (401, 403, 500).
  - Request logging (debug mode).
  - Authentication (if needed).
- Type-safe API responses using TypeScript interfaces.

## State Management
- Use Svelte stores for global state:
  - `pipelineStatus`: Current pipeline status.
  - `files`: List of scanned files.
  - `settings`: User settings.
- Local storage for persisting settings.

## Real-time Updates
- Dashboard updates in real-time via SSE.
- Review queue updates automatically when files are processed.
- Progress bars update smoothly without flickering.
- Log viewer auto-scrolls to latest entries.

## Accessibility
- ARIA labels for all interactive elements.
- Keyboard navigation support (Tab, Enter, Esc).
- Screen reader compatibility.
- Color contrast compliance (WCAG 2.1).

## Responsive Design
- Mobile-first approach.
- Breakpoints: sm (640px), md (768px), lg (1024px), xl (1280px).
- Collapsible sidebar on mobile.
- Touch-friendly buttons and inputs.