import { create } from 'zustand';
import type { IdentifiedEvent, VestigeEvent, VestigeEventType } from '@/types';
import { queryClient } from './query';

const MAX_EVENTS = 200;
let _eventIdCounter = 0;

/**
 * Map a server-side `VestigeEvent` to the TanStack Query keys it should
 * invalidate. The keys here use prefix matching (TanStack Query default), so
 * `['memories']` matches every `queryKeys.memories(params)` cache line.
 *
 * Without this, the dashboard relied on staleTime + refetch-on-mount to pick
 * up server-side mutations triggered from MCP tools or other clients, which
 * left stale lists visible for up to 30 seconds. The cost of over-invalidating
 * is one extra refetch per event — TanStack Query batches these natively.
 */
const INVALIDATIONS_BY_EVENT: Partial<Record<VestigeEventType, ReadonlyArray<readonly unknown[]>>> = {
  MemoryCreated: [['memories'], ['stats'], ['retentionDistribution'], ['graph'], ['timeline']],
  MemoryUpdated: [['memories'], ['memory'], ['stats']],
  MemoryDeleted: [['memories'], ['memory'], ['stats'], ['retentionDistribution'], ['graph']],
  MemoryPromoted: [['memory'], ['stats'], ['retentionDistribution']],
  MemoryDemoted: [['memory'], ['stats'], ['retentionDistribution']],
  ConnectionDiscovered: [['graph']],
  RetentionDecayed: [['stats'], ['retentionDistribution']],
  ConsolidationCompleted: [['stats'], ['retentionDistribution'], ['memories']],
  // After dream finishes, the cohort's retention/connections shift wholesale.
  DreamCompleted: [['stats'], ['retentionDistribution'], ['memories'], ['graph'], ['predictions']],
};

function invalidateForEvent(event: VestigeEvent): void {
  const keys = INVALIDATIONS_BY_EVENT[event.type];
  if (!keys) return;
  for (const queryKey of keys) {
    queryClient.invalidateQueries({ queryKey: [...queryKey] });
  }
}

interface WebSocketStore {
  connected: boolean;
  events: IdentifiedEvent[];
  memoryCount: number;
  avgRetention: number;
  /**
   * Whether a dream cycle is currently running. Toggled by `DreamStarted`
   * (true) and `DreamCompleted` (false) WebSocket events. Consumed by
   * `Graph3D` to drive the dream visual mode (bloom/fog/aurora). This is
   * single source of truth so dreams triggered from any client (this UI,
   * MCP tool, CLI) reflect on every connected dashboard.
   */
  isDreaming: boolean;
  clearEvents: () => void;
}

export const useWebSocket = create<WebSocketStore>((set) => ({
  connected: false,
  events: [],
  memoryCount: 0,
  avgRetention: 0,
  isDreaming: false,
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

/**
 * Process a raw WebSocket message payload and update the store.
 * Exported for unit testing — covers Heartbeat, dream lifecycle, and event
 * append behavior without needing a real WebSocket connection.
 *
 * Returns true when the message was successfully handled, false on parse error.
 */
export function processWebSocketMessage(rawData: string): boolean {
  try {
    const parsed: VestigeEvent = JSON.parse(rawData);
    if (parsed.type === 'Heartbeat') {
      useWebSocket.setState({
        memoryCount: (parsed.data?.memoryCount as number) ?? 0,
        avgRetention: (parsed.data?.avgRetention as number) ?? 0,
      });
      return true;
    }
    // Track dream lifecycle so the 3D graph can switch into dream visual mode.
    // Mutating isDreaming here (rather than deriving it in components) keeps
    // it consistent across every connected dashboard.
    if (parsed.type === 'DreamStarted') {
      useWebSocket.setState({ isDreaming: true });
    } else if (parsed.type === 'DreamCompleted') {
      useWebSocket.setState({ isDreaming: false });
    }
    invalidateForEvent(parsed);
    const identified: IdentifiedEvent = { ...parsed, _id: ++_eventIdCounter };
    useWebSocket.setState((s) => ({
      events: [identified, ...s.events].slice(0, MAX_EVENTS),
    }));
    return true;
  } catch {
    // biome-ignore lint/suspicious/noConsole: surfacing malformed wire payloads is a debugging affordance
    console.warn('[vestige] Malformed WebSocket message:', rawData);
    return false;
  }
}

function connect() {
  if (ws?.readyState === WebSocket.OPEN) return;

  ws = new WebSocket(getWsUrl());

  ws.onopen = () => {
    reconnectAttempts = 0;
    useWebSocket.setState({ connected: true });
  };

  ws.onmessage = (event) => {
    processWebSocketMessage(event.data);
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
