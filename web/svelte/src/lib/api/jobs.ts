import { request } from './request';
import type { Job } from '$lib/types';

export const jobsApi = {
  list() {
    return request<Job[]>('/v1/jobs');
  },

  get(id: string) {
    return request<Job>(`/v1/jobs/${id}`);
  },

  cancel(id: string) {
    return request<void>(`/v1/jobs/${id}/cancel`, { method: 'POST' });
  },
};
