/**
 * Decide which node labels are worth rendering this frame.
 *
 * Without LOD the renderer crowds the canvas with up to N label sprites at
 * once — when N=300 the screen turns into noise and the labels obscure the
 * graph structure that motivated showing them in the first place. This
 * picker enforces a soft cap (`maxLabels`) while always preserving the
 * essentials:
 *   - the hovered node,
 *   - the selected node,
 *   - the keyboard-focused node,
 *   - nodes in the active focus 1-hop set (so context survives a hover).
 *
 * Beyond that, the remaining budget is filled by retention rank (strongest
 * memories label first). Far candidates are filtered by squared distance
 * to avoid pulling labels from across the scene that nobody can read anyway.
 *
 * Pure function — no Three.js dependency — so it lives next to the rest of
 * the graph helpers but stays unit-testable.
 */
export interface LabelCandidate {
  id: string;
  retention: number;
  /** Squared distance to camera in world units. Caller computes (cheaper than sqrt). */
  distSq: number;
}

export interface PickVisibleLabelsOptions {
  hoveredId: string | null;
  selectedId: string | null;
  /** Keyboard-cursor focus id (separate from selection — they can disagree). */
  focusedId: string | null;
  /**
   * Ids that should remain visible because they are connected to the
   * focused / hovered / selected node. Always shown regardless of distance,
   * so the user can read "what is the hovered node linked to".
   */
  focusConnected: ReadonlySet<string>;
  /** Soft cap on total visible labels. Essentials may exceed it. */
  maxLabels: number;
  /** Distance² beyond which candidates are dropped (unless essential). */
  maxDistSq: number;
}

export function pickVisibleLabels(
  candidates: readonly LabelCandidate[],
  opts: PickVisibleLabelsOptions,
): ReadonlySet<string> {
  const visible = new Set<string>();

  // Essentials — always on. Distance check explicitly bypassed so a node
  // the user is interacting with never disappears.
  if (opts.hoveredId) visible.add(opts.hoveredId);
  if (opts.selectedId) visible.add(opts.selectedId);
  if (opts.focusedId) visible.add(opts.focusedId);
  for (const id of opts.focusConnected) {
    visible.add(id);
  }

  // Fill remaining budget with strongest in-range memories.
  // Deterministic sort: retention desc, id asc on ties — keeps results
  // stable across frames so we don't flicker labels on and off as Map
  // iteration order shifts.
  const remaining = opts.maxLabels - visible.size;
  if (remaining > 0) {
    const inRange: LabelCandidate[] = [];
    for (const c of candidates) {
      if (c.distSq > opts.maxDistSq) continue;
      if (visible.has(c.id)) continue;
      inRange.push(c);
    }
    inRange.sort((a, b) => {
      if (a.retention !== b.retention) return b.retention - a.retention;
      return a.id < b.id ? -1 : 1;
    });
    const take = Math.min(remaining, inRange.length);
    for (let i = 0; i < take; i++) {
      visible.add(inRange[i].id);
    }
  }

  return visible;
}
