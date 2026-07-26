import { request } from './request';
import type { MoveStatus } from '$lib/types';

export const moveApi = {
  start(options: Record<string, unknown> = {}) {
    return request('/api/move', {
      method: 'POST',
      body: JSON.stringify(options),
    });
  },

  status() {
    return request<MoveStatus>('/api/move/status');
  },
};
