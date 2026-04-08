import { create } from 'zustand';
import type { IdentifiedEvent, VestigeEvent } from '@/types';

const MAX_EVENTS = 200;
let _eventIdCounter = 0;

interface WebSocketStore {
  connected: boolean;
  events: IdentifiedEvent[];
  memoryCount: number;
  avgRetention: number;
  clearEvents: () => void;
}

export const useWebSocket = create<WebSocketStore>((set) => ({
  connected: false,
  events: [],
  memoryCount: 0,
  avgRetention: 0,
  clearEvents: () => set({ events: [] }),
}));

let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectAttempts = 0;

function getWsUrl(): string {
  return window.location.port === '5173'
    ? `ws://${window.location.hostname}:3927/ws`
    : `ws://${window.location.host}/ws`;
}

function scheduleReconnect() {
  if (reconnectTimer) clearTimeout(reconnectTimer);
  const delay = Math.min(1000 * 2 ** reconnectAttempts, 30_000);
  reconnectAttempts++;
  reconnectTimer = setTimeout(connect, delay);
}

function connect() {
  if (ws?.readyState === WebSocket.OPEN) return;

  ws = new WebSocket(getWsUrl());

  ws.onopen = () => {
    reconnectAttempts = 0;
    useWebSocket.setState({ connected: true });
  };

  ws.onmessage = (event) => {
    try {
      const parsed: VestigeEvent = JSON.parse(event.data);
      if (parsed.type === 'Heartbeat') {
        useWebSocket.setState({
          memoryCount: (parsed.data?.memory_count as number) ?? 0,
          avgRetention: (parsed.data?.avg_retention as number) ?? 0,
        });
        return;
      }
      const identified: IdentifiedEvent = { ...parsed, _id: ++_eventIdCounter };
      useWebSocket.setState((s) => ({
        events: [identified, ...s.events].slice(0, MAX_EVENTS),
      }));
    } catch {
      // ignore malformed
    }
  };

  ws.onclose = () => {
    useWebSocket.setState({ connected: false });
    scheduleReconnect();
  };

  ws.onerror = () => {
    /* reconnect handles recovery */
  };
}

export function initWebSocket() {
  connect();
  return () => {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    ws?.close();
    ws = null;
  };
}
