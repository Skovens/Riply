import { lastEvent } from '$lib/stores';

// ── Event Data Types ────────────────────────────────────────

export type RipProgressData = {
  pct?: number;
  eta?: number | null;
  stage?: string;
};

export type RipCompleteData = {
  dest?: string;
};

export type RipErrorData = {
  message?: string;
};

export type RipMessageData = {
  text?: string;
};

export type RipNamedData = {
  title?: string;
  year?: number;
  type?: string;
};

export type JobData = {
  job_id?: string;
  processed?: number;
  total?: number;
  matched?: number;
  review?: number;
  moved?: number;
  errors?: number;
  [key: string]: unknown;
};

// ── Event Types ─────────────────────────────────────────────

export type SseEvent =
  // Drive events
  | { kind: 'drive'; action: 'inserted' | 'ejected' | 'removed'; index: number; data: Record<string, never> }
  // Rip events
  | { kind: 'rip'; action: 'starting'; index: number; data: Record<string, never> }
  | { kind: 'rip'; action: 'progress'; index: number; data: RipProgressData }
  | { kind: 'rip'; action: 'complete'; index: number; data: RipCompleteData }
  | { kind: 'rip'; action: 'error'; index: number; data: RipErrorData }
  | { kind: 'rip'; action: 'message'; index: number; data: RipMessageData }
  | { kind: 'rip'; action: 'cancelled'; index: number; data: Record<string, never> }
  | { kind: 'rip'; action: 'named'; index: number; data: RipNamedData }
  | { kind: 'rip'; action: 'naming_failed'; index: number; data: Record<string, never> }
  // Job events (scan, analyze, move)
  | { kind: 'scan' | 'analyze' | 'move'; action: 'starting' | 'progress' | 'complete' | 'cancelled' | 'failed'; index: number; data: JobData }
  // Review events
  | { kind: 'review'; action: 'accepted' | 'rejected'; index: number; data: { id?: string } };

// ── Type Guards ─────────────────────────────────────────────

export function isRipEvent(event: SseEvent): event is Extract<SseEvent, { kind: 'rip' }> {
  return event.kind === 'rip';
}

export function isDriveEvent(event: SseEvent): event is Extract<SseEvent, { kind: 'drive' }> {
  return event.kind === 'drive';
}

export function isScanEvent(event: SseEvent): event is Extract<SseEvent, { kind: 'scan' }> {
  return event.kind === 'scan';
}

export function isAnalyzeEvent(event: SseEvent): event is Extract<SseEvent, { kind: 'analyze' }> {
  return event.kind === 'analyze';
}

export function isMoveEvent(event: SseEvent): event is Extract<SseEvent, { kind: 'move' }> {
  return event.kind === 'move';
}

export function isReviewEvent(event: SseEvent): event is Extract<SseEvent, { kind: 'review' }> {
  return event.kind === 'review';
}

export function isRipProgress(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'progress' }> {
  return event.kind === 'rip' && event.action === 'progress';
}

export function isRipComplete(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'complete' }> {
  return event.kind === 'rip' && event.action === 'complete';
}

export function isRipError(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'error' }> {
  return event.kind === 'rip' && event.action === 'error';
}

export function isRipMessage(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'message' }> {
  return event.kind === 'rip' && event.action === 'message';
}

export function isRipNamed(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'named' }> {
  return event.kind === 'rip' && event.action === 'named';
}

export function isRipNamingFailed(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'naming_failed' }> {
  return event.kind === 'rip' && event.action === 'naming_failed';
}

export function isRipStarting(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'starting' }> {
  return event.kind === 'rip' && event.action === 'starting';
}

export function isRipCancelled(event: SseEvent): event is Extract<SseEvent, { kind: 'rip'; action: 'cancelled' }> {
  return event.kind === 'rip' && event.action === 'cancelled';
}

export function isJobEvent(event: SseEvent): boolean {
  return ['scan', 'analyze', 'move'].includes(event.kind) && ['starting', 'progress', 'complete', 'cancelled', 'failed'].includes(event.action);
}

// ── SSE Client ──────────────────────────────────────────────

export type SseOptions = {
  onError?: (event: SseEvent | null, raw: string) => void;
};

export function connectSSE(onMessage: (event: SseEvent) => void, options?: SseOptions) {
  const eventSource = new EventSource('/v1/sse');

  const parseAndForward = (raw: string) => {
    try {
      const parsed = JSON.parse(raw) as SseEvent;
      onMessage(parsed);
      lastEvent.set(parsed);
    } catch (e) {
      console.error('SSE parse error:', e, raw);
      options?.onError?.(null, raw);
    }
  };

  eventSource.addEventListener('status', (event: MessageEvent) => {
    parseAndForward(event.data);
  });

  eventSource.onmessage = (event) => {
    parseAndForward(event.data);
  };

  eventSource.onerror = (error) => {
    console.error('SSE connection error:', error);
  };

  return () => eventSource.close();
}
