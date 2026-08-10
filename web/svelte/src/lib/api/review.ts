import { request } from './request';
import type { ReviewItem } from '$lib/types';

export const reviewApi = {
  list() {
    return request<ReviewItem[]>('/v1/review');
  },

  accept(id: string) {
    return request<void>(`/v1/review/${id}/accept`, { method: 'POST' });
  },

  reject(id: string) {
    return request<void>(`/v1/review/${id}/reject`, { method: 'POST' });
  },
};
