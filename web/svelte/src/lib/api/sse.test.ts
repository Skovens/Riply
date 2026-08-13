import { describe, it, expect } from 'vitest';
import { parseEvent, ripEventIndex, ripProgressFields, ripMessageText } from './parse-event';

describe('parseEvent', () => {
  it('parses simple kind:action', () => {
    expect(parseEvent('drive:changed')).toEqual({
      kind: 'drive',
      action: 'changed',
      detail: '',
    });
  });

  it('parses kind:action with detail', () => {
    expect(parseEvent('rip:starting:0')).toEqual({
      kind: 'rip',
      action: 'starting',
      detail: '0',
    });
  });

  it('parses rip:progress with index, pct, and eta', () => {
    expect(parseEvent('rip:progress:0:50:120')).toEqual({
      kind: 'rip',
      action: 'progress',
      detail: '0:50:120',
    });
  });

  it('parses rip:complete', () => {
    expect(parseEvent('rip:complete:0')).toEqual({
      kind: 'rip',
      action: 'complete',
      detail: '0',
    });
  });

  it('parses rip:error with message containing colons', () => {
    expect(parseEvent('rip:error:2:Drive not found: /dev/sr0')).toEqual({
      kind: 'rip',
      action: 'error',
      detail: '2:Drive not found: /dev/sr0',
    });
  });

  it('returns null for data with no colon separator', () => {
    expect(parseEvent('malformed')).toBeNull();
  });

  it('returns null for empty string', () => {
    expect(parseEvent('')).toBeNull();
  });

  it('parses scan:progress', () => {
    expect(parseEvent('scan:progress:5:10')).toEqual({
      kind: 'scan',
      action: 'progress',
      detail: '5:10',
    });
  });

  it('parses scan:complete', () => {
    expect(parseEvent('scan:complete')).toEqual({
      kind: 'scan',
      action: 'complete',
      detail: '',
    });
  });

  it('parses move:complete', () => {
    expect(parseEvent('move:complete')).toEqual({
      kind: 'move',
      action: 'complete',
      detail: '',
    });
  });
});

describe('ripEventIndex', () => {
  it('extracts index from simple detail', () => {
    expect(ripEventIndex('0')).toBe(0);
  });

  it('extracts index from progress detail', () => {
    expect(ripEventIndex('0:50:120')).toBe(0);
  });

  it('extracts index from error detail with message', () => {
    expect(ripEventIndex('2:Drive not found')).toBe(2);
  });

  it('returns NaN for empty string', () => {
    expect(Number.isNaN(ripEventIndex(''))).toBe(true);
  });
});

describe('ripProgressFields', () => {
  it('extracts index, pct, eta from valid detail', () => {
    const result = ripProgressFields('0:50:120');
    expect(result).toEqual({ index: 0, pct: 50, eta: 120 });
  });

  it('clamps pct to 0-100 range', () => {
    expect(ripProgressFields('0:-5:10').pct).toBe(0);
    expect(ripProgressFields('0:150:10').pct).toBe(100);
  });

  it('handles missing eta', () => {
    expect(ripProgressFields('0:50')).toEqual({ index: 0, pct: 50, eta: null });
  });

  it('handles zero pct', () => {
    expect(ripProgressFields('0:0')).toEqual({ index: 0, pct: 0, eta: null });
  });

  it('handles non-numeric strings returning 0', () => {
    const result = ripProgressFields('abc:def');
    expect(result.index).toBeNaN();
    expect(result.pct).toBe(0);
    expect(result.eta).toBeNull();
  });
});

describe('ripMessageText', () => {
  it('extracts message after index', () => {
    expect(ripMessageText('0:Getting disc info…')).toBe('Getting disc info…');
  });

  it('extracts multi-word message', () => {
    expect(ripMessageText('0:Scanning title 5 of 12')).toBe('Scanning title 5 of 12');
  });

  it('handles empty message', () => {
    expect(ripMessageText('0:')).toBe('');
  });
});

describe('rip:message event integration', () => {
  it('rip:message payload parseEvent + ripMessageText round-trips', () => {
    const raw = 'rip:message:0:Getting disc info…';
    const event = parseEvent(raw);
    expect(event).not.toBeNull();
    expect(event!.kind).toBe('rip');
    expect(event!.action).toBe('message');
    expect(event!.detail).toBe('0:Getting disc info…');
    expect(ripEventIndex(event!.detail)).toBe(0);
    expect(ripMessageText(event!.detail)).toBe('Getting disc info…');
  });
});
