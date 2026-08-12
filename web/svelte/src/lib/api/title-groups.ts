import { request } from './request';
import type { TitleGroup } from '$lib/types';

export interface NameTitleGroupPayload {
  title: string;
  year?: number;
  media_type?: string;
  series_name?: string;
}

export const titleGroupsApi = {
  list() {
    return request<TitleGroup[]>('/v1/title-groups');
  },

  name(id: string, data: NameTitleGroupPayload) {
    return request<TitleGroup>(`/v1/title-groups/${id}/name`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  },
};
