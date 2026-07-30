import { request } from './request';
import type { SearchResult } from '$lib/types';

export const searchApi = {
  search(query: string, type: 'movie' | 'tv' = 'movie'): Promise<SearchResult[]> {
    return request<SearchResult[]>(
      `/api/search?q=${encodeURIComponent(query)}&type=${type}`,
    );
  },
};
