import { act, renderHook } from '@testing-library/react';
import { useReducedMotion } from './use-reduced-motion';

interface MockMql {
  matches: boolean;
  listeners: Set<(e: MediaQueryListEvent) => void>;
  addEventListener: (type: 'change', cb: (e: MediaQueryListEvent) => void) => void;
  removeEventListener: (type: 'change', cb: (e: MediaQueryListEvent) => void) => void;
  dispatch: (matches: boolean) => void;
}

function createMockMql(initial: boolean): MockMql {
  const listeners = new Set<(e: MediaQueryListEvent) => void>();
  const mql: MockMql = {
    matches: initial,
    listeners,
    addEventListener: (_type, cb) => {
      listeners.add(cb);
    },
    removeEventListener: (_type, cb) => {
      listeners.delete(cb);
    },
    dispatch: (matches: boolean) => {
      mql.matches = matches;
      for (const cb of listeners) {
        cb({ matches } as MediaQueryListEvent);
      }
    },
  };
  return mql;
}

function installMatchMedia(initial: boolean): MockMql {
  const mql = createMockMql(initial);
  vi.stubGlobal(
    'matchMedia',
    vi.fn((query: string) => {
      // Only stub the reduced-motion query; if anything else is queried, return inert default.
      if (query.includes('prefers-reduced-motion')) return mql;
      return { ...createMockMql(false) };
    }),
  );
  return mql;
}

describe('useReducedMotion', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns false when the user has not requested reduced motion', () => {
    installMatchMedia(false);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(false);
  });

  it('returns true when the user has requested reduced motion at startup', () => {
    installMatchMedia(true);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(true);
  });

  it('reacts to OS-level changes during the session', () => {
    const mql = installMatchMedia(false);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(false);

    act(() => {
      mql.dispatch(true);
    });
    expect(result.current).toBe(true);

    act(() => {
      mql.dispatch(false);
    });
    expect(result.current).toBe(false);
  });

  it('removes its listener on unmount (no leak)', () => {
    const mql = installMatchMedia(false);
    const { unmount } = renderHook(() => useReducedMotion());
    expect(mql.listeners.size).toBe(1);
    unmount();
    expect(mql.listeners.size).toBe(0);
  });
});
