/**
 * Pure parsing utilities for SSE event data — no Svelte dependencies.
 * Kept separate from sse.ts so they are directly testable in vitest
 * without requiring the $lib/stores alias.
 */

import type { SseEvent } from './sse';

/**
 * Type guard: is this a rip event?
 */
export function isRipEvent(event: SseEvent): boolean {
  return event.kind === 'rip';
}

/**
 * Type guard: is this a drive event?
 */
export function isDriveEvent(event: SseEvent): boolean {
  return event.kind === 'drive';
}

/**
 * Type guard: is this a scan event?
 */
export function isScanEvent(event: SseEvent): boolean {
  return event.kind === 'scan';
}

/**
 * Type guard: is this an analyze event?
 */
export function isAnalyzeEvent(event: SseEvent): boolean {
  return event.kind === 'analyze';
}

/**
 * Type guard: is this a move event?
 */
export function isMoveEvent(event: SseEvent): boolean {
  return event.kind === 'move';
}

/**
 * Type guard: is this a review event?
 */
export function isReviewEvent(event: SseEvent): boolean {
  return event.kind === 'review';
}

/**
 * Type guard: is this a rip progress event?
 */
export function isRipProgress(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'progress';
}

/**
 * Type guard: is this a rip complete event?
 */
export function isRipComplete(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'complete';
}

/**
 * Type guard: is this a rip error event?
 */
export function isRipError(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'error';
}

/**
 * Type guard: is this a rip message event?
 */
export function isRipMessage(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'message';
}

/**
 * Type guard: is this a rip named event?
 */
export function isRipNamed(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'named';
}

/**
 * Type guard: is this a rip naming_failed event?
 */
export function isRipNamingFailed(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'naming_failed';
}

/**
 * Type guard: is this a rip starting event?
 */
export function isRipStarting(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'starting';
}

/**
 * Type guard: is this a rip cancelled event?
 */
export function isRipCancelled(event: SseEvent): boolean {
  return event.kind === 'rip' && event.action === 'cancelled';
}

/**
 * Extract the drive index from a parsed SSE event.
 */
export function ripEventIndex(event: SseEvent): number {
  return event.index;
}

/**
 * Extract progress fields from a rip progress event.
 */
export function ripProgressFields(event: SseEvent): {
  index: number;
  pct: number;
  eta: number | null;
  stage: string;
} {
  const data = event.data as { pct?: unknown; eta?: unknown; stage?: string };
  const rawPct = data.pct;
  const pct = typeof rawPct === 'number' && !isNaN(rawPct) ? rawPct : parseInt(String(rawPct ?? ''), 10);
  const rawEta = data.eta;
  const eta = rawEta === null || rawEta === undefined ? null : (typeof rawEta === 'number' && !isNaN(rawEta) ? rawEta : parseInt(String(rawEta), 10));
  return {
    index: event.index,
    pct: Math.min(100, Math.max(0, isNaN(pct) ? 0 : pct)),
    eta: isNaN(eta as number) ? null : eta,
    stage: data.stage || '',
  };
}

/**
 * Extract the message text from a rip message event.
 */
export function ripMessageText(event: SseEvent): string {
  return (event.data as { text?: string }).text || '';
}

/**
 * Extract the destination path from a rip complete event.
 */
export function ripCompleteDest(event: SseEvent): string {
  return (event.data as { dest?: string }).dest || '';
}

/**
 * Extract the error message from a rip error event.
 */
export function ripErrorMessage(event: SseEvent): string {
  return (event.data as { message?: string }).message || '';
}

/**
 * Extract the title info from a rip named event.
 */
export function ripNamedInfo(event: SseEvent): { title: string; year?: number; type: string } {
  const data = event.data as { title?: string; year?: number; type?: string };
  return {
    title: data.title || '',
    year: data.year,
    type: data.type || 'movie',
  };
}
