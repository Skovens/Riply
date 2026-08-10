import { request } from './request';
import type { DriveInfo, InstallationStatus } from '$lib/types';

export const ripperApi = {
  getStatus() {
    return request<InstallationStatus>('/v1/ripper/status');
  },

  listDrives() {
    return request<DriveInfo[]>('/v1/ripper/drives');
  },

  start(index: number, destination: string) {
    return request(`/v1/ripper/${index}/start`, {
      method: 'POST',
      body: JSON.stringify({ destination }),
    });
  },

  cancel(index: number) {
    return request<void>(`/v1/ripper/${index}/cancel`, {
      method: 'POST',
    });
  },

  getAutoripStatus() {
    return request<number[]>('/v1/ripper/autorip');
  },

  toggleAutorip(index: number) {
    return request<number[]>(`/v1/ripper/autorip/${index}/toggle`, {
      method: 'POST',
    });
  },
};
