import { request } from './request';

export const scanApi = {
  start(directory: string) {
    return request('/v1/scan', {
      method: 'POST',
      body: JSON.stringify({ directory }),
    });
  },
};
