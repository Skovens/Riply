import { lastEvent } from '$lib/stores';

export type SseEvent = {
  kind: string;
  action: string;
  index: number;
  data: Record<string, unknown>;
};

export function connectSSE(onMessage: (event: SseEvent) => void) {
  const eventSource = new EventSource('/v1/sse');

  eventSource.addEventListener('status', (event: MessageEvent) => {
    try {
      const parsed = JSON.parse(event.data) as SseEvent;
      onMessage(parsed);
      lastEvent.set(parsed);
    } catch (e) {
      console.error('SSE parse error:', e, event.data);
    }
  });

  eventSource.onmessage = (event) => {
    try {
      const parsed = JSON.parse(event.data) as SseEvent;
      onMessage(parsed);
      lastEvent.set(parsed);
    } catch (e) {
      console.error('SSE parse error:', e, event.data);
    }
  };

  eventSource.onerror = (error) => {
    console.error('SSE error:', error);
  };

  return () => eventSource.close();
}
