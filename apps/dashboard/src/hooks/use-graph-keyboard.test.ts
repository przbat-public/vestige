import { act, renderHook } from '@testing-library/react';
import { useGraphKeyboard } from './use-graph-keyboard';

type Node = { id: string; content: string };

const NODES: Node[] = [
  { id: 'a', content: 'alpha' },
  { id: 'b', content: 'beta' },
  { id: 'c', content: 'gamma' },
];

interface KeyOpts {
  altKey?: boolean;
  shiftKey?: boolean;
}

function makeKey(key: string, modifiers: KeyOpts = {}): React.KeyboardEvent<HTMLDivElement> {
  return {
    key,
    altKey: modifiers.altKey ?? false,
    shiftKey: modifiers.shiftKey ?? false,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  } as unknown as React.KeyboardEvent<HTMLDivElement>;
}

describe('useGraphKeyboard', () => {
  it('returns no focused node initially', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    expect(result.current.focusedNodeId).toBeNull();
  });

  it('ArrowDown from null focuses the first node', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => {
      result.current.onKeyDown(makeKey('ArrowDown'));
    });
    expect(result.current.focusedNodeId).toBe('a');
  });

  it('ArrowDown wraps from last to first', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('c');
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('a');
  });

  it('ArrowUp from null focuses the last node', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowUp')));
    expect(result.current.focusedNodeId).toBe('c');
  });

  it('ArrowUp wraps from first to last', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('a');
    act(() => result.current.onKeyDown(makeKey('ArrowUp')));
    expect(result.current.focusedNodeId).toBe('c');
  });

  it('Home jumps to first, End to last', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('End')));
    expect(result.current.focusedNodeId).toBe('c');
    act(() => result.current.onKeyDown(makeKey('Home')));
    expect(result.current.focusedNodeId).toBe('a');
  });

  it('Enter selects the focused node', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    act(() => result.current.onKeyDown(makeKey('Enter')));
    expect(onSelect).toHaveBeenCalledWith('a');
  });

  it('Space selects the focused node', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    act(() => result.current.onKeyDown(makeKey(' ')));
    expect(onSelect).toHaveBeenCalledWith('b');
  });

  it('Enter without a focused node is a no-op', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('Enter')));
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('Escape clears the focused node', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('a');
    act(() => result.current.onKeyDown(makeKey('Escape')));
    expect(result.current.focusedNodeId).toBeNull();
  });

  it('Escape invokes the optional onEscape callback so parents can close drawers', () => {
    // Pre-fix: useGraphKeyboard owned Escape but only cleared its own
    // focus state — the open MemoryDetail drawer in GraphPage kept its
    // visible state, leaving the UI in a contradiction where focus was
    // "cleared" yet the detail panel still showed the previous node.
    // The callback lets the parent piggy-back the same key without
    // re-binding window listeners and racing the hook.
    const onSelect = vi.fn();
    const onEscape = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect, { onEscape }));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    act(() => result.current.onKeyDown(makeKey('Escape')));
    expect(onEscape).toHaveBeenCalledTimes(1);
    expect(result.current.focusedNodeId).toBeNull();
  });

  it('Escape with no onEscape provided still clears focus (back-compat)', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('Home')));
    act(() => result.current.onKeyDown(makeKey('Escape')));
    expect(result.current.focusedNodeId).toBeNull();
  });

  it('ignores keys it does not handle', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    const evt = makeKey('a');
    act(() => result.current.onKeyDown(evt));
    expect(result.current.focusedNodeId).toBeNull();
    expect(evt.preventDefault).not.toHaveBeenCalled();
  });

  it('prevents default on handled keys to avoid scroll', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    const evt = makeKey('ArrowDown');
    act(() => result.current.onKeyDown(evt));
    expect(evt.preventDefault).toHaveBeenCalled();
  });

  it('survives an empty node list without crashing', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard([] as Node[], onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBeNull();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('drops focus when the focused node disappears from the list', () => {
    const onSelect = vi.fn();
    const { result, rerender } = renderHook(({ nodes }) => useGraphKeyboard(nodes, onSelect), {
      initialProps: { nodes: NODES },
    });
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('b');
    rerender({ nodes: NODES.filter((n) => n.id !== 'b') });
    expect(result.current.focusedNodeId).toBeNull();
  });

  it('exposes container ARIA props (role, tabIndex, aria-activedescendant)', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(NODES, onSelect));
    expect(result.current.containerProps.role).toBe('application');
    expect(result.current.containerProps.tabIndex).toBe(0);
    expect(result.current.containerProps['aria-activedescendant']).toBeUndefined();
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.containerProps['aria-activedescendant']).toBe('graph-node-a');
  });
});

describe('useGraphKeyboard — spatial neighbour mode', () => {
  // Layout: a left, b right, c top, d bottom. ArrowRight from `a` must land on `b`,
  // not on whatever happens to come after `a` in the nodes array.
  const SPATIAL_NODES: Node[] = [
    { id: 'a', content: 'left' },
    { id: 'b', content: 'right' },
    { id: 'c', content: 'top' },
    { id: 'd', content: 'bottom' },
  ];
  const positions = new Map<string, { x: number; y: number }>([
    ['a', { x: -0.6, y: 0 }],
    ['b', { x: 0.6, y: 0 }],
    ['c', { x: 0, y: 0.6 }],
    ['d', { x: 0, y: -0.6 }],
  ]);

  it('ArrowRight with positions picks the spatial neighbour to the right', () => {
    const onSelect = vi.fn();
    const getScreenPositions = vi.fn(() => positions);
    const { result } = renderHook(() => useGraphKeyboard(SPATIAL_NODES, onSelect, { getScreenPositions }));
    // First ArrowRight from null focus seeds at leftmost ('a')
    act(() => result.current.onKeyDown(makeKey('ArrowRight')));
    expect(result.current.focusedNodeId).toBe('a');
    // Second ArrowRight moves a → b (the spatial neighbour)
    act(() => result.current.onKeyDown(makeKey('ArrowRight')));
    expect(result.current.focusedNodeId).toBe('b');
  });

  it('ArrowUp moves to node above in screen space (not array order)', () => {
    const onSelect = vi.fn();
    const getScreenPositions = vi.fn(() => positions);
    const { result } = renderHook(() => useGraphKeyboard(SPATIAL_NODES, onSelect, { getScreenPositions }));
    // Seed focus at d (bottom) — pickAnchorForDirection('up') = bottom.
    act(() => result.current.onKeyDown(makeKey('ArrowUp')));
    expect(result.current.focusedNodeId).toBe('d');
    // Then up moves toward c (top).
    act(() => result.current.onKeyDown(makeKey('ArrowUp')));
    expect(result.current.focusedNodeId).toBe('c');
  });

  it('does NOT use spatial mode when getScreenPositions returns an empty map (loading state)', () => {
    const onSelect = vi.fn();
    const getScreenPositions = vi.fn(() => new Map<string, { x: number; y: number }>());
    const { result } = renderHook(() => useGraphKeyboard(SPATIAL_NODES, onSelect, { getScreenPositions }));
    // Fallback: array order behaviour, ArrowDown picks first array element.
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('a');
  });

  it('arrow keys with Alt modifier are NOT consumed (parent can handle history)', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(SPATIAL_NODES, onSelect));
    // Seed focus first
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('a');

    const evt = makeKey('ArrowLeft', { altKey: true });
    act(() => result.current.onKeyDown(evt));
    // No focus change — parent owns Alt+Arrow.
    expect(result.current.focusedNodeId).toBe('a');
    // No preventDefault either — otherwise the parent can't see the event.
    expect(evt.preventDefault).not.toHaveBeenCalled();
  });

  it('arrow keys with Cmd/Ctrl/Shift also fall through', () => {
    const onSelect = vi.fn();
    const { result } = renderHook(() => useGraphKeyboard(SPATIAL_NODES, onSelect));
    act(() => result.current.onKeyDown(makeKey('ArrowDown')));
    expect(result.current.focusedNodeId).toBe('a');

    const cmd = makeKey('ArrowRight', { altKey: false }); // simulate meta
    (cmd as unknown as { metaKey: boolean }).metaKey = true;
    act(() => result.current.onKeyDown(cmd));
    expect(result.current.focusedNodeId).toBe('a');
    expect(cmd.preventDefault).not.toHaveBeenCalled();
  });
});
