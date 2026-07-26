import { request } from './request';
import type { Config } from '$lib/types';

export const configApi = {
  get() {
    return request<Config>('/api/config');
  },

  update(data: Partial<Config>) {
    return request<Config>('/api/config', {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  },
};
