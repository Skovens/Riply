import { request } from './request';
import type { Job } from '$lib/types';

export const jobsApi = {
  list() {
    return request<Job[]>('/api/jobs');
  },

  get(id: string) {
    return request<Job>(`/api/jobs/${id}`);
  },

  cancel(id: string) {
    return request<void>(`/api/jobs/${id}/cancel`, { method: 'POST' });
  },
};
