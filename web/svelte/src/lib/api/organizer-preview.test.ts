import { describe, it, expect, vi, beforeEach } from 'vitest';
import { organizerApi, type FolderRecord } from './organizer';
import { request } from './request';

// Mock the request module
vi.mock('./request', () => ({
  request: vi.fn(),
}));

describe('organizerApi.previewMove', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('calls the correct endpoint', async () => {
    const mockResponse = { success: true, data: { destination: '/media/movies/Inception (2010)/' } };
    (request as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse.data);

    await organizerApi.previewMove('folder-123');

    expect(request).toHaveBeenCalledWith('/v1/folders/folder-123/preview');
  });

  it('returns destination from response', async () => {
    const expectedDestination = '/media/movies/Inception (2010)/';
    (request as ReturnType<typeof vi.fn>).mockResolvedValue({ destination: expectedDestination });

    const result = await organizerApi.previewMove('folder-123');

    expect(result).toEqual({ destination: expectedDestination });
  });

  it('throws on API error', async () => {
    (request as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('Not found'));

    await expect(organizerApi.previewMove('nonexistent')).rejects.toThrow('Not found');
  });
});

describe('organizerApi.accept', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('calls the correct endpoint', async () => {
    const mockFolder: FolderRecord = {
      id: 'folder-123',
      path: '/data/test',
      name: 'Test',
      file_count: 1,
      extra_files: [],
      status: 'Confident',
      created_at: '2024-01-01',
      updated_at: '2024-01-01',
    };
    (request as ReturnType<typeof vi.fn>).mockResolvedValue(mockFolder);

    await organizerApi.accept('folder-123');

    expect(request).toHaveBeenCalledWith('/v1/folders/folder-123/accept', {
      method: 'POST',
    });
  });
});

describe('organizerApi.reject', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('calls the correct endpoint', async () => {
    const mockFolder: FolderRecord = {
      id: 'folder-123',
      path: '/data/test',
      name: 'Test',
      file_count: 1,
      extra_files: [],
      status: 'Unknown',
      created_at: '2024-01-01',
      updated_at: '2024-01-01',
    };
    (request as ReturnType<typeof vi.fn>).mockResolvedValue(mockFolder);

    await organizerApi.reject('folder-123');

    expect(request).toHaveBeenCalledWith('/v1/folders/folder-123/reject', {
      method: 'POST',
    });
  });
});
