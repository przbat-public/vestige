import { act, renderHook } from '@testing-library/react';
import { useMultiSelect } from './use-multi-select';

interface Item {
  id: string;
  label: string;
}

const items: Item[] = [
  { id: 'a', label: 'A' },
  { id: 'b', label: 'B' },
  { id: 'c', label: 'C' },
  { id: 'd', label: 'D' },
];

const getId = (it: Item) => it.id;

describe('useMultiSelect', () => {
  it('starts empty', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    expect(result.current.count).toBe(0);
    expect(result.current.hasSelection).toBe(false);
    expect(result.current.allSelected).toBe(false);
    expect(result.current.partiallySelected).toBe(false);
  });

  it('toggles a single id on/off', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    act(() => result.current.toggle('a'));
    expect(result.current.isSelected('a')).toBe(true);
    expect(result.current.count).toBe(1);
    act(() => result.current.toggle('a'));
    expect(result.current.isSelected('a')).toBe(false);
    expect(result.current.count).toBe(0);
  });

  it('shift-click selects the range from anchor to target inclusive', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    act(() => result.current.toggle('a')); // anchor = a
    act(() => result.current.toggle('c', { shift: true })); // a..c

    expect(result.current.isSelected('a')).toBe(true);
    expect(result.current.isSelected('b')).toBe(true);
    expect(result.current.isSelected('c')).toBe(true);
    expect(result.current.isSelected('d')).toBe(false);
  });

  it('shift-click works in reverse direction (target before anchor)', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    act(() => result.current.toggle('d')); // anchor = d
    act(() => result.current.toggle('b', { shift: true })); // d..b → b,c,d

    expect(result.current.isSelected('a')).toBe(false);
    expect(result.current.isSelected('b')).toBe(true);
    expect(result.current.isSelected('c')).toBe(true);
    expect(result.current.isSelected('d')).toBe(true);
  });

  it('shift-click without an anchor falls back to single toggle', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    act(() => result.current.toggle('c', { shift: true }));
    expect(result.current.isSelected('c')).toBe(true);
    expect(result.current.count).toBe(1);
  });

  it('selectAll fills the set; clear empties it and resets the anchor', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    act(() => result.current.selectAll());
    expect(result.current.count).toBe(4);
    expect(result.current.allSelected).toBe(true);

    act(() => result.current.clear());
    expect(result.current.count).toBe(0);

    // After clear, shift-click without toggling first behaves like a single
    // toggle (anchor was reset) — this guards against stale anchors leaking
    // across selection sessions.
    act(() => result.current.toggle('c', { shift: true }));
    expect(result.current.count).toBe(1);
  });

  it('reports partiallySelected when some-but-not-all visible items are selected', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    act(() => result.current.toggle('a'));
    expect(result.current.partiallySelected).toBe(true);
    expect(result.current.allSelected).toBe(false);
  });

  it('selectedItems returns items in input order, filtered by membership', () => {
    const { result } = renderHook(() => useMultiSelect(items, getId));
    act(() => result.current.toggle('c'));
    act(() => result.current.toggle('a'));
    expect(result.current.selectedItems.map((it) => it.id)).toEqual(['a', 'c']);
  });

  it('handles items list changes — selectAll uses the latest list', () => {
    const { result, rerender } = renderHook(({ list }: { list: Item[] }) => useMultiSelect(list, getId), {
      initialProps: { list: items.slice(0, 2) },
    });
    act(() => result.current.selectAll());
    expect(result.current.count).toBe(2);

    rerender({ list: items });
    act(() => result.current.selectAll());
    expect(result.current.count).toBe(4);
  });
});
