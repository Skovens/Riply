import { request } from './request';
import type { FileRecord, PaginatedResponse } from '$lib/types';

export const filesApi = {
  list(params: { status?: string; search?: string; page?: number; per_page?: number } = {}) {
    const query = new URLSearchParams();
    if (params.status) query.set('status', params.status);
    if (params.search) query.set('search', params.search);
    if (params.page) query.set('page', String(params.page));
    if (params.per_page) query.set('per_page', String(params.per_page));

    const qs = query.toString();
    return request<PaginatedResponse<FileRecord>>(`/v1/files${qs ? '?' + qs : ''}`);
  },

  get(id: string) {
    return request<FileRecord>(`/v1/files/${id}`);
  },

  update(id: string, data: Record<string, unknown>) {
    return request<FileRecord>(`/v1/files/${id}`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  },

  delete(id: string) {
    return request<void>(`/v1/files/${id}`, {
      method: 'DELETE',
    });
  },

  reset() {
    return request<number>('/v1/files', {
      method: 'DELETE',
    });
  },
};
