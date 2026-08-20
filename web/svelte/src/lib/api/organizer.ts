import { request } from './request';
import type { FolderRecord, FolderStatus } from '$lib/types';

export interface FolderScanResult {
  folder_count: number;
  folders: FolderRecord[];
}

export interface FolderStatsResponse {
  by_status: [string, number][];
}

export interface MovePreviewResponse {
  destination: string;
}

export const organizerApi = {
  /** List all folders, optionally filtered by status */
  list(status?: FolderStatus) {
    const url = status ? `/v1/folders?status=${status}` : '/v1/folders';
    return request<FolderRecord[]>(url);
  },

  /** Get folder statistics by status */
  stats() {
    return request<FolderStatsResponse>('/v1/folders/stats');
  },

  /** Scan a directory for folders containing media files */
  scan(directory: string) {
    return request<FolderScanResult>('/v1/folders/scan', {
      method: 'POST',
      body: JSON.stringify({ directory }),
    });
  },

  /** Get a single folder by ID */
  get(id: string) {
    return request<FolderRecord>(`/v1/folders/${id}`);
  },

  /** Analyze a folder — run TMDb search and update record */
  analyze(id: string) {
    return request<FolderRecord>(`/v1/folders/${id}/analyze`, {
      method: 'POST',
    });
  },

  /** Accept a folder — move to Confident status */
  accept(id: string) {
    return request<FolderRecord>(`/v1/folders/${id}/accept`, {
      method: 'POST',
    });
  },

  /** Reject a folder — move to Unknown status */
  reject(id: string) {
    return request<FolderRecord>(`/v1/folders/${id}/reject`, {
      method: 'POST',
    });
  },

  /** Accept all confident folders */
  acceptAll() {
    return request<number>('/v1/folders/accept-all', {
      method: 'POST',
    });
  },

  /** Reject all unknown folders */
  rejectAll() {
    return request<number>('/v1/folders/reject-all', {
      method: 'POST',
    });
  },

  /** Move a single confident folder to its destination */
  move(id: string) {
    return request<void>(`/v1/folders/${id}/move`, {
      method: 'POST',
    });
  },

  /** Preview where a folder would move to */
  previewMove(id: string) {
    return request<MovePreviewResponse>(`/v1/folders/${id}/preview`);
  },

  /** Delete a folder by ID */
  delete(id: string) {
    return request<void>(`/v1/folders/${id}`, {
      method: 'DELETE',
    });
  },
};
