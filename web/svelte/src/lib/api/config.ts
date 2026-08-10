import { request } from './request';
import type { Config } from '$lib/types';

export const configApi = {
  get() {
    return request<Config>('/v1/config');
  },

  update(data: Partial<Config>) {
    return request<Config>('/v1/config', {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  },

  reset() {
    return request<Config>('/v1/config/reset', {
      method: 'POST',
    });
  },
};
