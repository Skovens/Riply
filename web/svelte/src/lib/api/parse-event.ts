/**
 * Pure parsing utilities for SSE event data — no Svelte dependencies.
 * Kept separate from sse.ts so they are directly testable in vitest
 * without requiring the $lib/stores alias.
 */

import type {
  SseEvent,
  RipProgressData,
  RipCompleteData,
  RipErrorData,
  RipMessageData,
  RipNamedData,
} from './sse';

// ── Re-export type guards from sse.ts (for test isolation) ──

export function isRipEvent(event: SseEvent): boolean {
  return event.kind === 'rip';
}

export function isDriveEvent(event: SseEvent): boolean {
  return event.kind === 'drive';
}

export function isScanEvent(event: SseEvent): boolean {
  return event.kind === 'scan';
}

export function isAnalyzeEvent(event: SseEvent): boolean {
  return event.kind === 'analyze';
}

export function isMoveEvent(event: SseEvent): boolean {
  return event.kind === 'move';
}

export function isReviewEvent(event: SseEvent): boolean {
  return event.kind === 'review';
}

export function isRipProgress(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'progress';
}

export function isRipComplete(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'complete';
}

export function isRipError(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'error';
}

export function isRipMessage(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'message';
}

export function isRipNamed(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'named';
}

export function isRipNamingFailed(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'naming_failed';
}

export function isRipStarting(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'starting';
}

export function isRipCancelled(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'cancelled';
}

// ── Extraction helpers ──────────────────────────────────────

/** Extract the drive index from a parsed SSE event. */
export function ripEventIndex(event: SseEvent): number {
  return event.index;
}

/** Extract progress fields from a rip progress event. */
export function ripProgressFields(event: Extract<SseEvent, { kind: 'rip'; action: 'progress' }>): {
  index: number;
  pct: number;
  eta: number | null;
  stage: string;
} {
  const data = event.data as RipProgressData;
  return {
    index: event.index,
    pct: typeof data.pct === 'number' ? Math.min(100, Math.max(0, data.pct)) : 0,
    eta: data.eta ?? null,
    stage: data.stage || '',
  };
}

/** Extract the message text from a rip message event. */
export function ripMessageText(event: Extract<SseEvent, { kind: 'rip'; action: 'message' }>): string {
  return (event.data as RipMessageData).text || '';
}

/** Extract the destination path from a rip complete event. */
export function ripCompleteDest(event: Extract<SseEvent, { kind: 'rip'; action: 'complete' }>): string {
  return (event.data as RipCompleteData).dest || '';
}

/** Extract the error message from a rip error event. */
export function ripErrorMessage(event: Extract<SseEvent, { kind: 'rip'; action: 'error' }>): string {
  return (event.data as RipErrorData).message || '';
}

/** Extract the title info from a rip named event. */
export function ripNamedInfo(event: Extract<SseEvent, { kind: 'rip'; action: 'named' }>): { title: string; year?: number; type: string } {
  const data = event.data as RipNamedData;
  return {
    title: data.title || '',
    year: data.year,
    type: data.type || 'movie',
  };
}
