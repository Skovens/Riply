import { request } from './request';

/** @typedef {import('$lib/types').SessionState} SessionState */

export const sessionStateApi = {
  /** @returns {Promise<SessionState[]>} */
  async list() {
    return request('/v1/session-state');
  },

  async clear() {
    await request('/v1/session-state', { method: 'DELETE' });
  },
};
