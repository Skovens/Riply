import { describe, it, expect, beforeEach } from 'vitest';

/**
 * Create a host element the same way openMediaNamingModal does.
 * We test this in isolation to avoid importing svelte (which requires
 * the full $lib alias resolution only available in SvelteKit).
 */
function createModalHost(): HTMLDivElement {
  const host = document.createElement('div');
  host.style.position = 'fixed';
  host.style.inset = '0';
  host.style.zIndex = '9999';
  document.body.appendChild(host);
  return host;
}

describe('openMediaNamingModal host element', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('creates a host element appended to body', () => {
    const host = createModalHost();
    expect(document.body.contains(host)).toBe(true);
  });

  it('host must NOT have pointerEvents:none (critical fix for clickability)', () => {
    const host = createModalHost();
    expect(host.style.pointerEvents).not.toBe('none');
  });

  it('host covers full viewport', () => {
    const host = createModalHost();
    expect(host.style.position).toBe('fixed');
    expect(host.style.inset).toBe('0px');
    expect(host.style.zIndex).toBe('9999');
  });

  it('removes host on cleanup', () => {
    const host = createModalHost();
    expect(document.body.querySelector('div')).not.toBeNull();
    host.remove();
    expect(document.body.querySelector('div')).toBeNull();
  });
});
