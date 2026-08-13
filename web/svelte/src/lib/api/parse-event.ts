/**
 * Pure parsing utilities for SSE event data — no Svelte dependencies.
 * Kept separate from sse.ts so they are directly testable in vitest
 * without requiring the $lib/stores alias.
 */

export interface ParsedSSEEvent {
  kind: string;
  action: string;
  detail: string;
}

/**
 * Parse an SSE status payload into structured parts.
 *
 * Expected format: `"kind:action:detail"` (e.g. `"rip:progress:0:50:120"`)
 * Returns null on unparseable input.
 */
export function parseEvent(data: string): ParsedSSEEvent | null {
  const parts = data.split(':');
  if (parts.length < 2) return null;
  return { kind: parts[0], action: parts[1], detail: parts.slice(2).join(':') };
}

/** Extract the drive index from a rip event detail string. */
export function ripEventIndex(detail: string): number {
  return parseInt(detail.split(':')[0], 10);
}

/** Extract progress fields from a `rip:progress` detail (`idx:pct:eta`). */
export function ripProgressFields(detail: string): { index: number; pct: number; eta: number | null } {
  const [indexStr, pctStr, etaStr] = detail.split(':');
  return {
    index: parseInt(indexStr, 10),
    pct: Math.min(100, Math.max(0, parseInt(pctStr, 10) || 0)),
    eta: etaStr ? parseInt(etaStr, 10) : null,
  };
}
