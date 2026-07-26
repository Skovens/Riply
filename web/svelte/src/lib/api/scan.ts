import { request } from './request';

export const scanApi = {
  start(directory: string) {
    return request('/api/scan', {
      method: 'POST',
      body: JSON.stringify({ directory }),
    });
  },
};
