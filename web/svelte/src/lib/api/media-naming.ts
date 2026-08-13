import { mount, unmount } from 'svelte';
import MediaNamingModal from '$lib/components/MediaNamingModal.svelte';

export interface MediaNamingChoice {
  title: string;
  year?: number;
  media_type: 'Movie' | 'Tv';
  series_name?: string;
  season_number?: number;
  episode_number?: number;
}

export interface MediaNamingSuggestion {
  title: string;
  year?: number;
}

export interface OpenMediaNamingOptions {
  /** Prefill for the search box / title field. */
  initialTitle?: string;
  /** Initial media type. Defaults to 'movie'. */
  mediaType?: 'movie' | 'tv';
  /** Auto-lookup used as a default suggestion (ADR-022: prefill, not commit). */
  suggest?: () => Promise<MediaNamingSuggestion | null>;
}

/**
 * Shared media-naming flow (ADR-022): mounts the reusable naming modal and
 * resolves with the chosen name (a picked match or a custom title), or `null`
 * when the user cancels. Callers apply the choice through their own endpoint.
 */
export async function openMediaNamingModal(
  options: OpenMediaNamingOptions = {},
): Promise<MediaNamingChoice | null> {
  return new Promise((resolve) => {
    const host = document.createElement('div');
    host.style.position = 'fixed';
    host.style.inset = '0';
    host.style.zIndex = '9999';
    document.body.appendChild(host);

    const app = mount(MediaNamingModal, {
      target: host,
      props: {
        initialTitle: options.initialTitle || '',
        mediaType: options.mediaType || 'movie',
        suggest: options.suggest,
        onConfirm: (choice: MediaNamingChoice) => {
          unmount(app);
          host.remove();
          resolve(choice);
        },
        onClose: () => {
          unmount(app);
          host.remove();
          resolve(null);
        },
      },
    });
  });
}
