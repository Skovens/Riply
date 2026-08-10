import { lastEvent } from '$lib/stores';

export function connectSSE(onMessage: (data: string) => void) {
  const eventSource = new EventSource('/v1/sse');

  eventSource.addEventListener('status', (event: MessageEvent) => {
    onMessage(event.data);
    lastEvent.set(event.data);
  });

  eventSource.onmessage = (event) => {
    onMessage(event.data);
    lastEvent.set(event.data);
  };

  eventSource.onerror = (error) => {
    console.error('SSE error:', error);
  };

  return () => eventSource.close();
}
