import { request } from './request';
import type { ReviewItem } from '$lib/types';

export const reviewApi = {
  list() {
    return request<ReviewItem[]>('/api/review');
  },

  accept(id: string) {
    return request<void>(`/api/review/${id}/accept`, { method: 'POST' });
  },

  reject(id: string) {
    return request<void>(`/api/review/${id}/reject`, { method: 'POST' });
  },
};
