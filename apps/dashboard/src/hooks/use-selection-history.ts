import { useCallback, useState } from 'react';

/**
 * Stack-based selection history with browser-style back/forward semantics.
 *
 * Why a custom hook (vs window.history): we want this scoped to the graph
 * page — back/forward there should walk the selection trail, not the URL.
 * The browser history is reserved for actual navigation events (page changes).
 *
 * Semantics:
 * - `push(id)` truncates any forward branch (like clicking a new link in
 *   the browser would).
 * - Consecutive pushes of the same id are deduplicated — clicking the
 *   already-selected node twice should not pollute the history.
 * - `back`/`forward` move a cursor inside the existing stack; they do not
 *   pop entries.
 */
export interface SelectionHistoryState {
  currentId: string | null;
  canGoBack: boolean;
  canGoForward: boolean;
  history: readonly string[];
  push: (id: string) => void;
  back: () => void;
  forward: () => void;
  clear: () => void;
}

interface InternalState {
  stack: string[];
  cursor: number; // -1 when stack is empty, otherwise 0..stack.length-1
}

interface Options {
  initialId?: string | null;
  /**
   * Maximum number of entries kept in history. When pushing past this limit
   * we drop the OLDEST entry. Default 50 — large enough to cover a typical
   * exploration session, small enough that React state diffs stay cheap.
   */
  maxEntries?: number;
}

function makeInitial(opts: Options): InternalState {
  if (opts.initialId) {
    return { stack: [opts.initialId], cursor: 0 };
  }
  return { stack: [], cursor: -1 };
}

export function useSelectionHistory(opts: Options = {}): SelectionHistoryState {
  const maxEntries = opts.maxEntries ?? 50;
  const [state, setState] = useState<InternalState>(() => makeInitial(opts));

  const push = useCallback(
    (id: string) => {
      setState((prev) => {
        // Dedupe consecutive identical pushes — common when clicking the
        // already-selected node, or when both the canvas click handler and
        // the keyboard Enter both fire for the same id.
        if (prev.cursor >= 0 && prev.stack[prev.cursor] === id) {
          return prev;
        }
        // Truncate forward branch — pushing after a back() invalidates
        // whatever the user had browsed beyond `cursor`.
        const truncated = prev.stack.slice(0, prev.cursor + 1);
        truncated.push(id);
        // Cap at maxEntries from the front (oldest dropped first).
        let stack = truncated;
        if (stack.length > maxEntries) {
          stack = stack.slice(stack.length - maxEntries);
        }
        return { stack, cursor: stack.length - 1 };
      });
    },
    [maxEntries],
  );

  const back = useCallback(() => {
    setState((prev) => {
      if (prev.cursor <= 0) return prev;
      return { ...prev, cursor: prev.cursor - 1 };
    });
  }, []);

  const forward = useCallback(() => {
    setState((prev) => {
      if (prev.cursor < 0 || prev.cursor >= prev.stack.length - 1) return prev;
      return { ...prev, cursor: prev.cursor + 1 };
    });
  }, []);

  const clear = useCallback(() => {
    setState({ stack: [], cursor: -1 });
  }, []);

  const currentId = state.cursor >= 0 ? (state.stack[state.cursor] ?? null) : null;
  const canGoBack = state.cursor > 0;
  const canGoForward = state.cursor >= 0 && state.cursor < state.stack.length - 1;

  return {
    currentId,
    canGoBack,
    canGoForward,
    history: state.stack,
    push,
    back,
    forward,
    clear,
  };
}
