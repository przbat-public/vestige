import { processWebSocketMessage, useWebSocket } from './websocket';

function resetStore() {
  useWebSocket.setState({
    connected: false,
    events: [],
    memoryCount: 0,
    avgRetention: 0,
    isDreaming: false,
  });
}

describe('processWebSocketMessage', () => {
  beforeEach(() => {
    resetStore();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('updates memoryCount and avgRetention on Heartbeat without appending to events', () => {
    const ok = processWebSocketMessage(
      JSON.stringify({ type: 'Heartbeat', data: { memoryCount: 42, avgRetention: 0.73 } }),
    );

    expect(ok).toBe(true);
    const state = useWebSocket.getState();
    expect(state.memoryCount).toBe(42);
    expect(state.avgRetention).toBe(0.73);
    expect(state.events).toEqual([]);
  });

  it('flips isDreaming to true on DreamStarted and appends to events', () => {
    processWebSocketMessage(JSON.stringify({ type: 'DreamStarted', data: {} }));

    const state = useWebSocket.getState();
    expect(state.isDreaming).toBe(true);
    expect(state.events).toHaveLength(1);
    expect(state.events[0].type).toBe('DreamStarted');
  });

  it('flips isDreaming back to false on DreamCompleted', () => {
    processWebSocketMessage(JSON.stringify({ type: 'DreamStarted', data: {} }));
    expect(useWebSocket.getState().isDreaming).toBe(true);

    processWebSocketMessage(JSON.stringify({ type: 'DreamCompleted', data: {} }));
    expect(useWebSocket.getState().isDreaming).toBe(false);
  });

  it('does not toggle isDreaming for unrelated event types', () => {
    processWebSocketMessage(JSON.stringify({ type: 'DreamStarted', data: {} }));
    expect(useWebSocket.getState().isDreaming).toBe(true);

    processWebSocketMessage(JSON.stringify({ type: 'MemoryCreated', data: { id: 'abc' } }));
    expect(useWebSocket.getState().isDreaming).toBe(true);
  });

  it('appends events newest-first and caps at MAX_EVENTS', () => {
    for (let i = 0; i < 205; i++) {
      processWebSocketMessage(
        JSON.stringify({ type: 'MemoryCreated', data: { seq: i } }),
      );
    }

    const events = useWebSocket.getState().events;
    expect(events.length).toBe(200);
    // Newest event is first; oldest 5 dropped.
    expect((events[0].data as { seq: number }).seq).toBe(204);
    expect((events[199].data as { seq: number }).seq).toBe(5);
  });

  it('returns false and does not throw on malformed JSON', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const ok = processWebSocketMessage('not-json{');
    expect(ok).toBe(false);
    expect(useWebSocket.getState().events).toEqual([]);
    expect(warn).toHaveBeenCalled();
  });
});
