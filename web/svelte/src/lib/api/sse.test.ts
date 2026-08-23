import { describe, it, expect } from 'vitest';
import {
  isRipProgress,
  isRipComplete,
  isRipError,
  isRipMessage,
  isRipNamed,
  isRipNamingFailed,
  isRipStarting,
  isRipCancelled,
  isDriveEvent,
  isScanEvent,
  isAnalyzeEvent,
  isMoveEvent,
  isReviewEvent,
  ripEventIndex,
  ripProgressFields,
  ripMessageText,
  ripCompleteDest,
  ripErrorMessage,
  ripNamedInfo,
} from './parse-event';
import type { SseEvent } from './sse';

// ── Helpers ──────────────────────────────────────────────────

function makeRipProgressEvent(overrides: Partial<SseEvent> = {}): Extract<SseEvent, { kind: 'rip'; action: 'progress' }> {
  return {
    kind: 'rip',
    action: 'progress',
    index: 0,
    data: {},
    ...overrides,
  } as Extract<SseEvent, { kind: 'rip'; action: 'progress' }>;
}

function makeEvent(overrides: Partial<SseEvent>): SseEvent {
  return {
    kind: 'rip',
    action: 'progress',
    index: 0,
    data: {},
    ...overrides,
  };
}

// ── Type guards ──────────────────────────────────────────────

describe('Type guards', () => {
  it('isRipProgress matches rip:progress', () => {
    expect(isRipProgress(makeRipProgressEvent())).toBe(true);
  });

  it('isRipProgress rejects rip:complete', () => {
    expect(isRipProgress(makeEvent({ kind: 'rip', action: 'complete' }))).toBe(false);
  });

  it('isRipComplete matches rip:complete', () => {
    expect(isRipComplete(makeEvent({ kind: 'rip', action: 'complete' }))).toBe(true);
  });

  it('isRipError matches rip:error', () => {
    expect(isRipError(makeEvent({ kind: 'rip', action: 'error' }))).toBe(true);
  });

  it('isRipMessage matches rip:message', () => {
    expect(isRipMessage(makeEvent({ kind: 'rip', action: 'message' }))).toBe(true);
  });

  it('isRipNamed matches rip:named', () => {
    expect(isRipNamed(makeEvent({ kind: 'rip', action: 'named' }))).toBe(true);
  });

  it('isRipNamingFailed matches rip:naming_failed', () => {
    expect(isRipNamingFailed(makeEvent({ kind: 'rip', action: 'naming_failed' }))).toBe(true);
  });

  it('isRipStarting matches rip:starting', () => {
    expect(isRipStarting(makeEvent({ kind: 'rip', action: 'starting' }))).toBe(true);
  });

  it('isRipCancelled matches rip:cancelled', () => {
    expect(isRipCancelled(makeEvent({ kind: 'rip', action: 'cancelled' }))).toBe(true);
  });

  it('isDriveEvent matches drive:inserted', () => {
    expect(isDriveEvent(makeEvent({ kind: 'drive', action: 'inserted' }))).toBe(true);
  });

  it('isScanEvent matches scan:starting', () => {
    expect(isScanEvent(makeEvent({ kind: 'scan', action: 'starting' }))).toBe(true);
  });

  it('isAnalyzeEvent matches analyze:progress', () => {
    expect(isAnalyzeEvent(makeEvent({ kind: 'analyze', action: 'progress' }))).toBe(true);
  });

  it('isMoveEvent matches move:complete', () => {
    expect(isMoveEvent(makeEvent({ kind: 'move', action: 'complete' }))).toBe(true);
  });

  it('isReviewEvent matches review:accepted', () => {
    expect(isReviewEvent(makeEvent({ kind: 'review', action: 'accepted' }))).toBe(true);
  });
});

// ── ripEventIndex ────────────────────────────────────────────

describe('ripEventIndex', () => {
  it('extracts index from event', () => {
    expect(ripEventIndex(makeEvent({ index: 0 }))).toBe(0);
  });

  it('extracts index 2 from drive event', () => {
    expect(ripEventIndex(makeEvent({ kind: 'drive', action: 'removed', index: 2 }))).toBe(2);
  });
});

// ── ripProgressFields ────────────────────────────────────────

describe('ripProgressFields', () => {
  it('extracts index, pct, eta from valid event', () => {
    const result = ripProgressFields(makeRipProgressEvent({
      index: 0,
      data: { pct: 50, eta: 120 },
    }));
    expect(result).toEqual({ index: 0, pct: 50, eta: 120, stage: '' });
  });

  it('clamps pct to 0-100 range', () => {
    expect(ripProgressFields(makeRipProgressEvent({ data: { pct: -5 } })).pct).toBe(0);
    expect(ripProgressFields(makeRipProgressEvent({ data: { pct: 150 } })).pct).toBe(100);
  });

  it('handles missing eta', () => {
    expect(ripProgressFields(makeRipProgressEvent({ data: { pct: 50 } })).eta).toBeNull();
  });

  it('handles zero pct', () => {
    expect(ripProgressFields(makeRipProgressEvent({ data: { pct: 0 } })).pct).toBe(0);
  });

  it('handles non-numeric pct returning 0', () => {
    const result = ripProgressFields(makeRipProgressEvent({ data: { pct: 'abc' as unknown as number } }));
    expect(result.pct).toBe(0);
  });

  it('extracts stage from data', () => {
    const result = ripProgressFields(makeRipProgressEvent({
      index: 1,
      data: { pct: 25, eta: 60, stage: 'Ripping title 3' },
    }));
    expect(result).toEqual({ index: 1, pct: 25, eta: 60, stage: 'Ripping title 3' });
  });
});

// ── ripMessageText ───────────────────────────────────────────

describe('ripMessageText', () => {
  it('extracts message text', () => {
    expect(ripMessageText(makeEvent({
      index: 0,
      kind: 'rip',
      action: 'message',
      data: { text: 'Getting disc info…' },
    }))).toBe('Getting disc info…');
  });

  it('extracts multi-word message', () => {
    expect(ripMessageText(makeEvent({
      index: 0,
      kind: 'rip',
      action: 'message',
      data: { text: 'Scanning title 5 of 12' },
    }))).toBe('Scanning title 5 of 12');
  });

  it('handles empty message', () => {
    expect(ripMessageText(makeEvent({ index: 0, kind: 'rip', action: 'message', data: {} }))).toBe('');
  });
});

// ── ripCompleteDest ──────────────────────────────────────────

describe('ripCompleteDest', () => {
  it('extracts destination path', () => {
    expect(ripCompleteDest(makeEvent({
      index: 0,
      kind: 'rip',
      action: 'complete',
      data: { dest: '/home/user/Movies/Inception (2010)' },
    }))).toBe('/home/user/Movies/Inception (2010)');
  });

  it('handles missing dest', () => {
    expect(ripCompleteDest(makeEvent({ index: 0, kind: 'rip', action: 'complete', data: {} }))).toBe('');
  });
});

// ── ripErrorMessage ──────────────────────────────────────────

describe('ripErrorMessage', () => {
  it('extracts error message', () => {
    expect(ripErrorMessage(makeEvent({
      index: 2,
      kind: 'rip',
      action: 'error',
      data: { message: 'Drive not found: /dev/sr0' },
    }))).toBe('Drive not found: /dev/sr0');
  });

  it('handles missing message', () => {
    expect(ripErrorMessage(makeEvent({ index: 0, kind: 'rip', action: 'error', data: {} }))).toBe('');
  });
});

// ── ripNamedInfo ─────────────────────────────────────────────

describe('ripNamedInfo', () => {
  it('extracts title, year, type', () => {
    const result = ripNamedInfo(makeEvent({
      index: 0,
      kind: 'rip',
      action: 'named',
      data: { title: 'Inception', year: 2010, type: 'movie' },
    }));
    expect(result).toEqual({ title: 'Inception', year: 2010, type: 'movie' });
  });

  it('handles missing year', () => {
    const result = ripNamedInfo(makeEvent({
      index: 0,
      kind: 'rip',
      action: 'named',
      data: { title: 'Braveheart' },
    }));
    expect(result).toEqual({ title: 'Braveheart', year: undefined, type: 'movie' });
  });

  it('handles missing type', () => {
    const result = ripNamedInfo(makeEvent({
      index: 0,
      kind: 'rip',
      action: 'named',
      data: { title: 'Breaking Bad', year: 2008 },
    }));
    expect(result).toEqual({ title: 'Breaking Bad', year: 2008, type: 'movie' });
  });
});

// ── JSON format robustness ───────────────────────────────────

describe('JSON format robustness', () => {
  it('rip:message with colon in text does not break parsing', () => {
    const event = makeEvent({
      index: 0,
      kind: 'rip',
      action: 'message',
      data: { text: 'Inception: The Movie' },
    });
    expect(ripMessageText(event)).toBe('Inception: The Movie');
  });

  it('rip:progress with all fields', () => {
    const event = makeRipProgressEvent({
      index: 1,
      data: { pct: 75, eta: 300, stage: 'Ripping title 8' },
    });
    expect(isRipProgress(event)).toBe(true);
    const fields = ripProgressFields(event);
    expect(fields).toEqual({ index: 1, pct: 75, eta: 300, stage: 'Ripping title 8' });
  });
});
