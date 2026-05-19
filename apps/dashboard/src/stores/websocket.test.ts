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
      // Use the real `MemoryCreated.data` shape (post-DTO refactor) so
      // the test type-checks against the discriminated union; the index
      // travels in `id`, which is already a string field.
      processWebSocketMessage(
        JSON.stringify({
          type: 'MemoryCreated',
          data: {
            id: `mem-${i}`,
            contentPreview: 'test',
            nodeType: 'fact',
            tags: [],
            timestamp: '2026-05-12T00:00:00Z',
          },
        }),
      );
    }

    const events = useWebSocket.getState().events;
    expect(events.length).toBe(200);
    // Newest event is first; oldest 5 dropped. `discriminate` narrows
    // the union — guard against the wrong variant first.
    const first = events[0];
    const last = events[199];
    if (first.type !== 'MemoryCreated' || last.type !== 'MemoryCreated') {
      throw new Error(`unexpected variants: ${first.type}, ${last.type}`);
    }
    expect(first.data.id).toBe('mem-204');
    expect(last.data.id).toBe('mem-5');
  });

  it('returns false and does not throw on malformed JSON', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const ok = processWebSocketMessage('not-json{');
    expect(ok).toBe(false);
    expect(useWebSocket.getState().events).toEqual([]);
    expect(warn).toHaveBeenCalled();
  });
});
