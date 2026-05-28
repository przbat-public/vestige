import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  type Direction,
  pickAnchorForDirection,
  type ScreenPos,
  selectSpatialNeighbor,
} from '@/graph/select-spatial-neighbor';

/**
 * Keyboard navigation for the 3D graph canvas.
 *
 * The renderer is a pure Three.js scene with no DOM tree for nodes, so we
 * model the keyboard cursor as a single "focused node id" tracked here.
 * The container exposes `role="application"` and an `aria-activedescendant`
 * that points at a virtual DOM id (`graph-node-<id>`) — GraphPage renders
 * matching hidden `<li>` elements so screen readers can announce focus
 * changes without us shoving the entire node list into the canvas div.
 *
 * Two navigation modes coexist:
 * - **Spatial mode** (preferred): if the caller passes `getScreenPositions`,
 *   arrow keys move to the nearest node in the requested screen-space
 *   direction. This is what users intuitively expect from a 3D viewer.
 * - **Array mode** (fallback): without screen positions, arrow keys walk
 *   the nodes array in insertion order. Used during loading or when the
 *   renderer cannot supply projection data (e.g. JSDOM tests).
 *
 * Modifier keys (Alt/Cmd/Ctrl/Shift) on arrow keys are intentionally
 * passed through as a no-op so the parent component can layer in higher-
 * level shortcuts (history back/forward on Alt+Arrow, edge traversal on
 * Shift+Arrow, etc.) without fighting this hook for `preventDefault`.
 *
 * Keys:
 * - ArrowDown/Right: next node (spatial or array order)
 * - ArrowUp/Left:    previous node
 * - Home/End:        first/last array entry (always array order — explicit jump)
 * - Enter/Space:     select focused node
 * - Escape:          clear focus
 */
export interface GraphKeyboardNode {
  id: string;
}

export interface GraphKeyboardResult {
  focusedNodeId: string | null;
  setFocusedNodeId: (id: string | null) => void;
  onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void;
  containerProps: {
    role: 'application';
    tabIndex: 0;
    'aria-activedescendant'?: string;
  };
}

export interface GraphKeyboardOptions {
  /**
   * Returns the current NDC-like screen positions for nodes. Called lazily
   * on each arrow key down — the caller is expected to project from world
   * coordinates through the active camera matrix at that moment, so we
   * always navigate against the actual visible layout.
   *
   * Return an empty map to opt out of spatial mode (e.g. while the scene
   * is still loading).
   */
  getScreenPositions?: () => ReadonlyMap<string, ScreenPos>;
  /**
   * Side-channel for Escape. The hook clears its own focus state, but
   * `GraphPage` also wants to close the MemoryDetail drawer when the
   * user dismisses navigation. Piggy-backing on this callback avoids
   * re-binding a window listener and racing the hook's preventDefault.
   *
   * Always fires after focus is cleared, so the parent reads a clean
   * state if it triggers a re-render.
   */
  onEscape?: () => void;
}

const NEXT_KEYS = new Set(['ArrowDown', 'ArrowRight']);
const PREV_KEYS = new Set(['ArrowUp', 'ArrowLeft']);
const SELECT_KEYS = new Set(['Enter', ' ']);
const HANDLED_KEYS = new Set([...NEXT_KEYS, ...PREV_KEYS, ...SELECT_KEYS, 'Home', 'End', 'Escape']);

function keyToDirection(key: string): Direction | null {
  switch (key) {
    case 'ArrowRight':
      return 'right';
    case 'ArrowLeft':
      return 'left';
    case 'ArrowUp':
      return 'up';
    case 'ArrowDown':
      return 'down';
    default:
      return null;
  }
}

export function useGraphKeyboard<T extends GraphKeyboardNode>(
  nodes: readonly T[],
  onSelect: (nodeId: string) => void,
  options: GraphKeyboardOptions = {},
): GraphKeyboardResult {
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const { getScreenPositions, onEscape } = options;

  // Drop focus when the focused node disappears from the list (e.g. the
  // user changed search/tag filter). Leaving a stale id would point
  // aria-activedescendant at nothing — screen readers go silent.
  useEffect(() => {
    if (focusedNodeId && !nodes.some((n) => n.id === focusedNodeId)) {
      setFocusedNodeId(null);
    }
  }, [nodes, focusedNodeId]);

  const onKeyDown = useCallback(
    // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: keyboard dispatcher branches over six handled keys plus modifier passthrough; splitting per-key would force shared mutable state through callback returns
    (event: React.KeyboardEvent<HTMLElement>) => {
      const key = event.key;
      if (!HANDLED_KEYS.has(key)) return;

      // Arrow keys with modifiers fall through to the parent (no
      // preventDefault, no state change) — they're reserved for higher-
      // level shortcuts like Alt+Arrow=history. Non-arrow handled keys
      // (Home/End/Enter/Space/Escape) are still consumed even with mods
      // because they have no conflicting browser shortcut.
      const direction = keyToDirection(key);
      const hasModifier = event.altKey || event.metaKey || event.ctrlKey || event.shiftKey;
      if (direction && hasModifier) return;

      if (nodes.length === 0) {
        event.preventDefault();
        return;
      }
      event.preventDefault();

      if (key === 'Escape') {
        setFocusedNodeId(null);
        onEscape?.();
        return;
      }
      if (key === 'Home') {
        setFocusedNodeId(nodes[0]?.id ?? null);
        return;
      }
      if (key === 'End') {
        setFocusedNodeId(nodes[nodes.length - 1]?.id ?? null);
        return;
      }
      if (SELECT_KEYS.has(key)) {
        if (focusedNodeId) onSelect(focusedNodeId);
        return;
      }

      if (!direction) return;

      // Spatial mode — only when positions are actually supplied.
      const positions = getScreenPositions?.();
      if (positions && positions.size > 0) {
        if (!focusedNodeId) {
          const anchor = pickAnchorForDirection(direction, positions);
          if (anchor) setFocusedNodeId(anchor);
          return;
        }
        const next = selectSpatialNeighbor(focusedNodeId, direction, positions);
        if (next) setFocusedNodeId(next);
        // If next === null (nothing in cone), keep focus where it is —
        // beats teleporting to the opposite side of the scene.
        return;
      }

      // Array-order fallback (legacy behaviour).
      const currentIdx = focusedNodeId ? nodes.findIndex((n) => n.id === focusedNodeId) : -1;
      if (NEXT_KEYS.has(key)) {
        const next = currentIdx < 0 ? 0 : (currentIdx + 1) % nodes.length;
        setFocusedNodeId(nodes[next]?.id ?? null);
        return;
      }
      if (PREV_KEYS.has(key)) {
        const prev = currentIdx <= 0 ? nodes.length - 1 : currentIdx - 1;
        setFocusedNodeId(nodes[prev]?.id ?? null);
      }
    },
    [nodes, focusedNodeId, onSelect, getScreenPositions, onEscape],
  );

  const containerProps = useMemo<GraphKeyboardResult['containerProps']>(() => {
    const props: GraphKeyboardResult['containerProps'] = {
      role: 'application' as const,
      tabIndex: 0 as const,
    };
    if (focusedNodeId) {
      props['aria-activedescendant'] = `graph-node-${focusedNodeId}`;
    }
    return props;
  }, [focusedNodeId]);

  return { focusedNodeId, setFocusedNodeId, onKeyDown, containerProps };
}
