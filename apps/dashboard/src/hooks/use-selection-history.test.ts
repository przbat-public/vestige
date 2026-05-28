import { act, renderHook } from '@testing-library/react';
import { useSelectionHistory } from './use-selection-history';

describe('useSelectionHistory', () => {
  it('starts with no current id and no back/forward capability', () => {
    const { result } = renderHook(() => useSelectionHistory());
    expect(result.current.currentId).toBeNull();
    expect(result.current.canGoBack).toBe(false);
    expect(result.current.canGoForward).toBe(false);
    expect(result.current.history).toEqual([]);
  });

  it('push() sets currentId and adds to history', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    expect(result.current.currentId).toBe('a');
    expect(result.current.history).toEqual(['a']);
    expect(result.current.canGoBack).toBe(false); // nothing to go back TO
    expect(result.current.canGoForward).toBe(false);
  });

  it('pushing a second id makes back-navigation possible', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    act(() => result.current.push('b'));
    expect(result.current.currentId).toBe('b');
    expect(result.current.canGoBack).toBe(true);
    expect(result.current.canGoForward).toBe(false);
  });

  it('back() moves the cursor backward and forward() restores it', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    act(() => result.current.push('b'));
    act(() => result.current.push('c'));

    act(() => result.current.back());
    expect(result.current.currentId).toBe('b');
    expect(result.current.canGoBack).toBe(true);
    expect(result.current.canGoForward).toBe(true);

    act(() => result.current.back());
    expect(result.current.currentId).toBe('a');
    expect(result.current.canGoBack).toBe(false);

    act(() => result.current.forward());
    expect(result.current.currentId).toBe('b');
    expect(result.current.canGoForward).toBe(true);
  });

  it('back() at the start is a no-op', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    act(() => result.current.back());
    expect(result.current.currentId).toBe('a');
  });

  it('forward() at the tip is a no-op', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    act(() => result.current.forward());
    expect(result.current.currentId).toBe('a');
  });

  it('push() after back() truncates the forward branch (browser semantics)', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    act(() => result.current.push('b'));
    act(() => result.current.push('c'));
    act(() => result.current.back()); // at b
    act(() => result.current.push('d')); // truncates c

    expect(result.current.currentId).toBe('d');
    expect(result.current.history).toEqual(['a', 'b', 'd']);
    expect(result.current.canGoForward).toBe(false);
  });

  it('pushing the same id consecutively is deduplicated', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    act(() => result.current.push('a'));
    expect(result.current.history).toEqual(['a']);
    expect(result.current.canGoBack).toBe(false);
  });

  it('clear() empties the stack', () => {
    const { result } = renderHook(() => useSelectionHistory());
    act(() => result.current.push('a'));
    act(() => result.current.push('b'));
    act(() => result.current.clear());
    expect(result.current.currentId).toBeNull();
    expect(result.current.history).toEqual([]);
  });

  it('caps history at the configured limit, dropping oldest first', () => {
    const { result } = renderHook(() => useSelectionHistory({ maxEntries: 3 }));
    act(() => result.current.push('a'));
    act(() => result.current.push('b'));
    act(() => result.current.push('c'));
    act(() => result.current.push('d'));
    expect(result.current.history).toEqual(['b', 'c', 'd']);
    expect(result.current.currentId).toBe('d');
  });

  it('respects an initial id passed to the hook', () => {
    const { result } = renderHook(() => useSelectionHistory({ initialId: 'seed' }));
    expect(result.current.currentId).toBe('seed');
    expect(result.current.history).toEqual(['seed']);
  });
});
