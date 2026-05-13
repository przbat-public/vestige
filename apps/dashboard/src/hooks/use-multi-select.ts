import { useCallback, useMemo, useRef, useState } from 'react';

interface MultiSelectAPI<T> {
  /** Identifiers of currently-selected items. */
  selected: Set<string>;
  /** Number of currently-selected items (memoized). */
  count: number;
  /** True when at least one item is selected — drives "selection mode" in UI. */
  hasSelection: boolean;
  /** True when all visible items are selected. */
  allSelected: boolean;
  /** True when at least one but not all visible items are selected. */
  partiallySelected: boolean;

  /**
   * Toggle a single id. Modifier flags follow the Linear/Notion convention:
   *
   * - `shift: true` — selects the range from the last anchor to this id
   *   (inclusive). The anchor is the last id you toggled without `shift`.
   * - `ctrl: true` (Cmd/Ctrl) — explicit single-toggle that does NOT clear
   *   the rest of the selection. We make this the default behavior of `toggle`
   *   anyway, but the flag is accepted for symmetry with future "select-only"
   *   click behavior if a caller wants it.
   *
   * Without modifiers, `toggle` flips the id's membership in the set.
   */
  toggle: (id: string, opts?: { shift?: boolean; ctrl?: boolean }) => void;

  /** Add an explicit range from the last anchor to `id`. No-op if no anchor. */
  selectRange: (id: string) => void;

  /** Set selection to the union of every visible id. */
  selectAll: () => void;

  /** Clear the entire selection and reset the anchor. */
  clear: () => void;

  /** Returns true when `id` is in the selected set. Stable identity for use in render. */
  isSelected: (id: string) => boolean;

  /** Returns the items the caller passed in, filtered by current selection. */
  selectedItems: T[];
}

/**
 * Multi-selection state with shift-click range + anchor tracking.
 *
 * Designed for list/grid UIs where users:
 *   - click an item → toggle just that item (anchor moves there)
 *   - shift-click → select every item between anchor and click target
 *   - press "Select all" → fill the set
 *   - press Escape → clear
 *
 * The hook is stateless about WHICH items exist — pass `items` in so
 * `selectAll`, `selectRange`, and `selectedItems` can resolve ids in document
 * order. `getId` is required because callers may have non-`id` keys (rare,
 * but it costs nothing to support).
 *
 * Why a Set: O(1) membership checks during render scale to thousands of items.
 * We deliberately reconstruct it on each mutation (immutable Set) so React's
 * referential equality detects the change.
 */
export function useMultiSelect<T>(items: T[], getId: (item: T) => string): MultiSelectAPI<T> {
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const anchorRef = useRef<string | null>(null);

  // Index lookup is stable per items reference; useMemo avoids rebuilding on
  // every render when callers pass the same array.
  const indexById = useMemo(() => {
    const m = new Map<string, number>();
    for (let i = 0; i < items.length; i++) m.set(getId(items[i]), i);
    return m;
  }, [items, getId]);

  const visibleIds = useMemo(() => items.map(getId), [items, getId]);

  const toggle = useCallback(
    (id: string, opts: { shift?: boolean; ctrl?: boolean } = {}) => {
      // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: range/shift/ctrl semantics intentionally live in one place to keep the selection state machine atomic
      setSelected((prev) => {
        const next = new Set(prev);

        if (opts.shift && anchorRef.current && anchorRef.current !== id) {
          // Range select: include every visible id between anchor and target.
          const aIdx = indexById.get(anchorRef.current);
          const bIdx = indexById.get(id);
          if (aIdx !== undefined && bIdx !== undefined) {
            const [from, to] = aIdx < bIdx ? [aIdx, bIdx] : [bIdx, aIdx];
            for (let i = from; i <= to; i++) next.add(visibleIds[i]);
            return next;
          }
        }

        // Single toggle. Update anchor regardless of add/remove so subsequent
        // shift-clicks anchor against the most-recent interaction.
        if (next.has(id)) next.delete(id);
        else next.add(id);
        anchorRef.current = id;
        return next;
      });
    },
    [indexById, visibleIds],
  );

  const selectRange = useCallback((id: string) => toggle(id, { shift: true }), [toggle]);

  const selectAll = useCallback(() => {
    setSelected(new Set(visibleIds));
  }, [visibleIds]);

  const clear = useCallback(() => {
    setSelected(new Set());
    anchorRef.current = null;
  }, []);

  const isSelected = useCallback((id: string) => selected.has(id), [selected]);

  const selectedItems = useMemo(() => items.filter((item) => selected.has(getId(item))), [items, selected, getId]);

  const visibleSet = useMemo(() => new Set(visibleIds), [visibleIds]);
  const visibleSelectedCount = useMemo(() => {
    let n = 0;
    for (const id of selected) if (visibleSet.has(id)) n++;
    return n;
  }, [selected, visibleSet]);

  return {
    selected,
    count: selected.size,
    hasSelection: selected.size > 0,
    allSelected: visibleIds.length > 0 && visibleSelectedCount === visibleIds.length,
    partiallySelected: visibleSelectedCount > 0 && visibleSelectedCount < visibleIds.length,
    toggle,
    selectRange,
    selectAll,
    clear,
    isSelected,
    selectedItems,
  };
}
