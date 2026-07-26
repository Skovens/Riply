/**
 * Riply API Client
 * 
 * Wrapper around fetch() for all backend API calls.
 * Returns parsed JSON responses or throws descriptive errors.
 */

const BASE_URL = '';

class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

/**
 * Generic fetch wrapper with error handling
 */
async function request(endpoint, options = {}) {
  const url = `${BASE_URL}${endpoint}`;
  
  const config = {
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
    ...options,
  };

  // Don't set Content-Type for GET/DELETE without body
  if (!options.body || options.method === 'GET' || options.method === 'DELETE') {
    delete config.headers['Content-Type'];
  }

  const response = await fetch(url, config);
  
  if (!response.ok) {
    const text = await response.text().catch(() => response.statusText);
    throw new ApiError(response.status, text);
  }

  return response.json();
}

/**
 * Health check
 */
export async function healthCheck() {
  const response = await fetch(`${BASE_URL}/health`);
  return response.ok;
}

/**
 * Files API
 */
export const files = {
  list(params = {}) {
    const query = new URLSearchParams();
    if (params.status) query.set('status', params.status);
    if (params.page) query.set('page', params.page);
    if (params.per_page) query.set('per_page', params.per_page);
    
    const qs = query.toString();
    return request(`/api/files${qs ? '?' + qs : ''}`);
  },

  get(id) {
    return request(`/api/files/${id}`);
  },

  update(id, data) {
    return request(`/api/files/${id}`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  },

  delete(id) {
    return request(`/api/files/${id}`, {
      method: 'DELETE',
    });
  },
};

/**
 * Review API
 */
export const review = {
  list() {
    return request('/api/review');
  },

  accept(id) {
    return request(`/api/review/${id}/accept`, { method: 'POST' });
  },

  reject(id) {
    return request(`/api/review/${id}/reject`, { method: 'POST' });
  },
};

/**
 * Scan API
 */
export const scan = {
  start(directory) {
    return request('/api/scan', {
      method: 'POST',
      body: JSON.stringify({ directory }),
    });
  },
};

/**
 * Analyze API
 */
export const analyze = {
  start() {
    return request('/api/analyze', { method: 'POST' });
  },
};

/**
 * Jobs API
 */
export const jobs = {
  list() {
    return request('/api/jobs');
  },

  get(id) {
    return request(`/api/jobs/${id}`);
  },

  cancel(id) {
    return request(`/api/jobs/${id}/cancel`, { method: 'POST' });
  },
};

/**
 * Move API
 */
export const move = {
  start(options = {}) {
    return request('/api/move', {
      method: 'POST',
      body: JSON.stringify(options),
    });
  },

  status() {
    return request('/api/move/status');
  },
};

/**
 * Ripper API
 */
export const ripper = {
  listDrives() {
    return request('/api/ripper/drives');
  },

  start(index, destination) {
    return request(`/api/ripper/${index}/start`, {
      method: 'POST',
      body: JSON.stringify({ destination }),
    });
  },

  cancel(index) {
    return request(`/api/ripper/${index}/cancel`, {
      method: 'POST',
    });
  },
};

/**
 * Config API
 */
export const config = {
  get() {
    return request('/api/config');
  },

  update(data) {
    return request('/api/config', {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  },
};

/**
 * SSE (Server-Sent Events) connection
 */
export function connectSSE(onMessage) {
  const eventSource = new EventSource(`${BASE_URL}/api/sse`);
  
  eventSource.onmessage = (event) => {
    onMessage(event.data);
  };

  eventSource.onerror = (error) => {
    console.error('SSE error:', error);
  };

  return () => eventSource.close();
}

export default {
  healthCheck,
  scan,
  analyze,
  jobs,
  files,
  review,
  move,
  ripper,
  config,
  connectSSE,
};
