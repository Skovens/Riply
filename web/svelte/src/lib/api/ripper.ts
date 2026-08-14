import { request } from './request';
import type { DriveInfo, InstallationStatus } from '$lib/types';
import type { MediaNamingChoice } from './media-naming';

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

  eject(index: number) {
    return request<void>(`/v1/ripper/${index}/eject`, {
      method: 'POST',
    });
  },

  rename(index: number, choice: MediaNamingChoice) {
    return request(`/v1/ripper/drives/${index}/rename`, {
      method: 'POST',
      body: JSON.stringify(choice),
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
