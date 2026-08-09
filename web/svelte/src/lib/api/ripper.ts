import { request } from './request';
import type { DriveInfo, InstallationStatus } from '$lib/types';

export const ripperApi = {
  getStatus() {
    return request<InstallationStatus>('/api/ripper/status');
  },

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

  getAutoripStatus() {
    return request<number[]>('/api/ripper/autorip');
  },

  toggleAutorip(index: number) {
    return request<number[]>(`/api/ripper/autorip/${index}/toggle`, {
      method: 'POST',
    });
  },
};
