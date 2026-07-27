import { request } from './request';
import type { MoveStatus } from '$lib/types';

export interface MovePreviewItem {
  id: string;
  filename: string;
  source_path: string;
  dest_path: string;
  media_type: string;
}

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

  preview() {
    return request<MovePreviewItem[]>('/api/move/preview');
  },
};
