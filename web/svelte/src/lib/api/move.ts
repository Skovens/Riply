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
    return request('/v1/move', {
      method: 'POST',
      body: JSON.stringify(options),
    });
  },

  status() {
    return request<MoveStatus>('/v1/move/status');
  },

  preview() {
    return request<MovePreviewItem[]>('/v1/move/preview');
  },
};
