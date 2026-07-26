import { request } from './request';
import type { DriveInfo } from '$lib/types';

export const ripperApi = {
  listDrives() {
    return request<DriveInfo[]>('/api/ripper/drives');
  },

  start(index: number, destination: string) {
    return request(`/api/ripper/${index}/start`, {
      method: 'POST',
      body: JSON.stringify({ destination }),
    });
  },

  cancel(index: number) {
    return request<void>(`/api/ripper/${index}/cancel`, {
      method: 'POST',
    });
  },
};
