import { describe, it, expect, vi, beforeEach } from 'vitest';
import { request, ApiError } from './request';
import { configApi } from './config';
import { filesApi } from './files';

function mockOkResponse(data: unknown) {
  return {
    ok: true,
    status: 200,
    json: () => Promise.resolve(data),
    text: () => Promise.resolve(''),
  } as Response;
}

function mockErrorResponse(status: number, text: string) {
  return {
    ok: false,
    status,
    text: () => Promise.resolve(text),
    json: () => Promise.reject(new Error('not json')),
  } as Response;
}

function mockWrappedResponse<T>(data: T) {
  return mockOkResponse({ success: true, data });
}

beforeEach(() => {
  vi.restoreAllMocks();
});

describe('request', () => {
  it('performs a GET request', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse('ok'));

    const result = await request<string>('/api/test');
    expect(result).toBe('ok');
    expect(global.fetch).toHaveBeenCalledWith('/api/test', expect.objectContaining({
      headers: expect.any(Object),
    }));
  });

  it('sends JSON body for POST', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse({ id: 1 }));

    const result = await request('/api/test', {
      method: 'POST',
      body: JSON.stringify({ key: 'value' }),
    });

    expect(result).toEqual({ id: 1 });
    const callArgs = (global.fetch as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(callArgs.method).toBe('POST');
    expect(callArgs.body).toBe('{"key":"value"}');
    expect(callArgs.headers!['Content-Type']).toBe('application/json');
  });

  it('removes Content-Type for GET without body', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse(null));

    await request('/api/test', { method: 'GET' });

    const callArgs = (global.fetch as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(callArgs.headers!['Content-Type']).toBeUndefined();
  });

  it('removes Content-Type for DELETE without body', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse(null));

    await request('/api/test', { method: 'DELETE' });

    const callArgs = (global.fetch as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(callArgs.headers!['Content-Type']).toBeUndefined();
  });

  it('throws ApiError on non-ok response', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockErrorResponse(404, 'Not Found'));

    await expect(request('/api/test')).rejects.toThrow(ApiError);
    await expect(request('/api/test')).rejects.toThrow('Not Found');
  });

  it('throws ApiError when ApiResponse.success is false', async () => {
    global.fetch = vi.fn().mockResolvedValue(
      mockOkResponse({ success: false, error: 'Validation failed', data: null }),
    );

    await expect(request('/api/test')).rejects.toThrow(ApiError);
    await expect(request('/api/test')).rejects.toThrow('Validation failed');
  });

  it('returns raw data when response is not wrapped', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockOkResponse({ raw: 'data' }));

    const result = await request('/api/test');
    expect(result).toEqual({ raw: 'data' });
  });
});

describe('configApi', () => {
  it('configApi.get calls GET /api/config', async () => {
    global.fetch = vi.fn().mockResolvedValue(
      mockWrappedResponse({ host: '0.0.0.0', port: 3000 }),
    );

    const result = await configApi.get();
    expect(result).toEqual({ host: '0.0.0.0', port: 3000 });
    expect(global.fetch).toHaveBeenCalledWith('/api/config', expect.any(Object));
  });

  it('configApi.update calls PUT /api/config with body', async () => {
    const response = { port: 8080, host: '0.0.0.0', database_path: '', movies_directory: '', tv_directory: '', verbose: false, ripping: { command: '' }, healthchecks: { uuid: '', ping_interval_minutes: 5 }, host: '', port: 8080 };
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse(response));

    const result = await configApi.update({ port: 8080 });
    expect(result.port).toBe(8080);
    const callArgs = (global.fetch as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(callArgs.method).toBe('PUT');
    expect(callArgs.body).toBe('{"port":8080}');
  });
});

describe('filesApi', () => {
  it('filesApi.list builds query string', async () => {
    const page = { items: [], total: 0, page: 1, per_page: 10 };
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse(page));

    await filesApi.list({ status: 'Scanned', page: 2, per_page: 10 });

    expect(global.fetch).toHaveBeenCalledWith(
      '/api/files?status=Scanned&page=2&per_page=10',
      expect.any(Object),
    );
  });

  it('filesApi.list omits query string when no params', async () => {
    global.fetch = vi.fn().mockResolvedValue(
      mockWrappedResponse({ items: [], total: 0, page: 1, per_page: 20 }),
    );

    await filesApi.list();

    expect(global.fetch).toHaveBeenCalledWith('/api/files', expect.any(Object));
  });

  it('filesApi.get calls GET /api/files/:id', async () => {
    global.fetch = vi.fn().mockResolvedValue(
      mockWrappedResponse({ id: 'abc', filename: 'test.mkv' }),
    );

    const result = await filesApi.get('abc');
    expect(result).toEqual({ id: 'abc', filename: 'test.mkv' });
    expect(global.fetch).toHaveBeenCalledWith('/api/files/abc', expect.any(Object));
  });

  it('filesApi.update calls POST /api/files/:id', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse({ id: 'abc' }));

    await filesApi.update('abc', { status: 'Confirmed' });

    const callArgs = (global.fetch as ReturnType<typeof vi.fn>).mock.calls[0][1];
    expect(callArgs.method).toBe('POST');
    expect(callArgs.body).toBe('{"status":"Confirmed"}');
  });

  it('filesApi.delete calls DELETE /api/files/:id', async () => {
    global.fetch = vi.fn().mockResolvedValue(mockWrappedResponse(null));

    await filesApi.delete('abc');

    expect(global.fetch).toHaveBeenCalledWith('/api/files/abc', expect.objectContaining({
      method: 'DELETE',
    }));
  });
});
