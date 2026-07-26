const BASE_URL = '';

export class ApiError extends Error {
  name = 'ApiError';
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export async function request<T = unknown>(endpoint: string, options: RequestInit = {}): Promise<T> {
  const url = `${BASE_URL}${endpoint}`;

  const config: RequestInit = {
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
    ...options,
  };

  // Don't set Content-Type for GET/DELETE without body
  if (!options.body || options.method === 'GET' || options.method === 'DELETE') {
    delete config.headers!['Content-Type'];
  }

  const response = await fetch(url, config);

  if (!response.ok) {
    const text = await response.text().catch(() => response.statusText);
    throw new ApiError(response.status, text);
  }

  return response.json() as Promise<T>;
}
