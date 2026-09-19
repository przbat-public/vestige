import { type QueryKey, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { BulkActionBar } from '@/components/memories/BulkActionBar';
import { MemoryDetail } from '@/components/memories/MemoryDetail';
import { MemoryListItem } from '@/components/memories/MemoryListItem';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { EmptyState } from '@/components/ui/empty-state';
import { NativeSelect } from '@/components/ui/native-select';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SearchInput } from '@/components/ui/search-input';
import { SkeletonList } from '@/components/ui/skeleton';
import { useMultiSelect } from '@/hooks/use-multi-select';
import { runWithConcurrency } from '@/lib/concurrency';
import { withoutMemories } from '@/lib/memory-envelope';
import { api } from '@/stores/api';
import { confirm } from '@/stores/confirm';
import { useDialogStore } from '@/stores/dialogs';
import { usePinned } from '@/stores/pinned';
import { queryKeys } from '@/stores/query';
import { clearRecentSearches, getRecentSearches, pushRecentSearch } from '@/stores/recent-searches';
import { EVENT, track, useTrackPageView } from '@/stores/telemetry';
import { toast } from '@/stores/toast';
import type { Memory } from '@/types';
import { NODE_TYPE_COLORS } from '@/types';

const memoryId = (m: Memory): string => m.id;

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: top-level page wires search, filters, multi-select, drawer, and CRUD; breaking it up further would scatter related state without clarifying flow
export function MemoriesPage() {
  useTrackPageView('memories');
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [query, setQuery] = useState('');
  const [activeQuery, setActiveQuery] = useState('');
  const [selected, setSelected] = useState<Memory | null>(null);
  const [typeFilter, setTypeFilter] = useState('');
  const [tagFilter, setTagFilter] = useState('');
  const [bulkBusy, setBulkBusy] = useState(false);

  // Page size for the list view. Backend clamps server-side to 1..200 so
  // raising this is cheap — we keep it at 100 to balance scroll length
  // against round-trip cost.
  const PAGE_SIZE = 100;
  const [pageOffset, setPageOffset] = useState(0);

  // Reset to page 0 whenever a filter changes — otherwise pagination
  // would point at an offset that no longer exists in the filtered set.
  // biome-ignore lint/correctness/useExhaustiveDependencies: filters are the change triggers
  useEffect(() => {
    setPageOffset(0);
  }, [typeFilter, tagFilter, activeQuery]);

  // Use `node_type` to match backend deserialization in `MemoryListParams`.
  // Sending `type` was a silent no-op for non-search list views — server only
  // accepts `node_type` (with a `type` serde alias for forward-compat).
  const listParams = {
    limit: String(PAGE_SIZE),
    offset: String(pageOffset),
    ...(typeFilter && { node_type: typeFilter }),
    ...(tagFilter && { tag: tagFilter }),
  };

  const {
    data: listData,
    isLoading: listLoading,
    isError: listError,
    error: listErrorObj,
    refetch: refetchList,
  } = useQuery({
    queryKey: queryKeys.memories(listParams),
    queryFn: () => api.memories.list(listParams),
    enabled: !activeQuery,
  });

  const {
    data: searchData,
    isLoading: searchLoading,
    isError: searchError,
    error: searchErrorObj,
    refetch: refetchSearch,
  } = useQuery({
    queryKey: queryKeys.search(activeQuery, 50),
    queryFn: () => api.search(activeQuery, 50),
    enabled: !!activeQuery,
  });

  const rawMemories = activeQuery ? (searchData?.results ?? []) : (listData?.memories ?? []);
  const pinned = usePinned();

  // Refresh the detail-panel snapshot when a list refetch returns a newer
  // version of the open memory. `selected` is a snapshot handed to
  // `MemoryDetail`, so without this the drawer keeps the pre-invalidation
  // copy after an edit (the mutation invalidates `['memories']`, which
  // refetches the active list query but never touched this state).
  useEffect(() => {
    setSelected((current) => {
      if (!current) return current;
      const fresh = rawMemories.find((m) => m.id === current.id);
      return fresh && fresh !== current ? fresh : current;
    });
  }, [rawMemories]);
  // Float pinned memories to the top while preserving the underlying order
  // for everything else. Stable: we partition rather than full-sort so a
  // search-ordered or recency-ordered list keeps its shape below the pins.
  const memories = useMemo(() => {
    if (pinned.size === 0) return rawMemories;
    const pinnedHits = rawMemories.filter((m) => pinned.has(m.id));
    const rest = rawMemories.filter((m) => !pinned.has(m.id));
    return [...pinnedHits, ...rest];
  }, [rawMemories, pinned]);
  const total = activeQuery ? (searchData?.total ?? 0) : (listData?.total ?? 0);
  const loading = activeQuery ? searchLoading : listLoading;
  const isError = activeQuery ? searchError : listError;
  const queryError = activeQuery ? searchErrorObj : listErrorObj;
  const retry = activeQuery ? refetchSearch : refetchList;

  const multi = useMultiSelect(memories, memoryId);

  const [recents, setRecents] = useState<string[]>(() => getRecentSearches());

  const handleSearch = () => {
    setActiveQuery(query);
    if (query.trim()) {
      track(EVENT.search_submit, { length: query.trim().length });
      pushRecentSearch(query);
      setRecents(getRecentSearches());
    }
  };

  const runRecent = (q: string) => {
    setQuery(q);
    setActiveQuery(q);
    pushRecentSearch(q);
    setRecents(getRecentSearches());
    track(EVENT.search_recent_use, { length: q.length });
  };

  const clearRecents = () => {
    clearRecentSearches();
    setRecents([]);
  };
  const invalidate = useCallback(() => {
    qc.invalidateQueries({ queryKey: ['memories'] });
    qc.invalidateQueries({ queryKey: ['search'] });
    qc.invalidateQueries({ queryKey: queryKeys.stats });
  }, [qc]);

  // "Open a specific memory" requests coming from outside the page
  // (⌘K hit, graph node click, etc.) land in `useDialogStore.pendingSelectMemoryId`.
  // We subscribe so the drawer opens whether the id was set before this
  // page mounted (palette → navigate flow) or while it's already open.
  // Fetching individually (vs scanning the cached list) means the memory
  // doesn't need to be in the current filter.
  const pendingSelectId = useDialogStore((s) => s.pendingSelectMemoryId);
  const consumeSelectMemory = useDialogStore((s) => s.consumeSelectMemory);
  useEffect(() => {
    if (!pendingSelectId) return;
    let cancelled = false;
    api.memories
      .get(pendingSelectId)
      .then((memory) => {
        if (!cancelled) setSelected(memory);
      })
      .catch(() => {
        // Don't surface a toast — the drawer simply won't open. Log so
        // dev tools still tell us what happened (e.g. id was deleted
        // between the palette selection and navigation landing).
        // biome-ignore lint/suspicious/noConsole: diagnostic in case the id is stale
        console.warn('[vestige] select-memory: failed to load', pendingSelectId);
      });
    // Clear the slot synchronously so a stale id can't replay if the
    // user navigates away and back.
    consumeSelectMemory();
    return () => {
      cancelled = true;
    };
  }, [pendingSelectId, consumeSelectMemory]);

  // Vim-style cursor for keyboard navigation through the list.
  // `null` = nothing focused yet; first `j` or `k` jumps to row 0.
  const [cursorIndex, setCursorIndex] = useState<number | null>(null);

  // Reset the cursor whenever the visible list changes shape (search,
  // filter, refetch). Without this, the cursor could index off the end
  // and `j`/`k` would feel broken. The deps are intentional triggers —
  // we don't *read* them, we listen for changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: filter values are the change triggers, not inputs
  useEffect(() => {
    setCursorIndex(null);
  }, [activeQuery, typeFilter, tagFilter]);

  // Esc clears selection, Cmd/Ctrl+A selects all visible, j/k move the
  // row cursor, Enter opens the focused row, x toggles its checkbox.
  // We register on the window so shortcuts work regardless of focus
  // location, but ignore events while typing — defined as TEXT inputs
  // and textareas. Checkbox / radio / button inputs are not considered
  // "typing" so the shortcuts still fire when focus is on a row
  // checkbox (Linear/Notion behavior).
  useEffect(() => {
    const isTypingTarget = (el: HTMLElement | null): boolean => {
      if (!el) return false;
      if (el.isContentEditable) return true;
      if (el.tagName === 'TEXTAREA') return true;
      if (el.tagName !== 'INPUT') return false;
      const type = (el as HTMLInputElement).type;
      return type !== 'checkbox' && type !== 'radio' && type !== 'button' && type !== 'submit';
    };
    // Selection shortcuts (Esc, ⌘A). Return value tells the caller whether
    // the event was consumed so the vim path below can short-circuit.
    const handleSelectionShortcut = (e: KeyboardEvent): boolean => {
      if (e.key === 'Escape' && multi.hasSelection) {
        multi.clear();
        return true;
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'a') {
        e.preventDefault();
        multi.selectAll();
        return true;
      }
      return false;
    };
    // Vim navigation — only fires when no modifier keys are held so we
    // never steal browser shortcuts like Cmd+J. Split into "move the
    // cursor" and "act on the cursor" to keep each helper small.
    const moveCursor = (key: string): boolean => {
      if (key === 'j' || key === 'ArrowDown') {
        setCursorIndex((i) => (i === null ? 0 : Math.min(memories.length - 1, i + 1)));
        return true;
      }
      if (key === 'k' || key === 'ArrowUp') {
        setCursorIndex((i) => (i === null ? 0 : Math.max(0, i - 1)));
        return true;
      }
      return false;
    };
    const actOnCursor = (key: string) => {
      if (cursorIndex === null) return;
      const m = memories[cursorIndex];
      if (!m) return;
      if (key === 'Enter') setSelected(m);
      else if (key === 'x') multi.toggle(m.id);
    };
    const handleVimNav = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (memories.length === 0) return;
      if (moveCursor(e.key)) {
        e.preventDefault();
        return;
      }
      actOnCursor(e.key);
    };
    const handler = (e: KeyboardEvent) => {
      if (isTypingTarget(e.target as HTMLElement | null)) return;
      if (handleSelectionShortcut(e)) return;
      handleVimNav(e);
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [multi, memories, cursorIndex]);

  // Follow the cursor with the drawer and keep the focused row in view.
  useEffect(() => {
    if (cursorIndex === null) return;
    const m = memories[cursorIndex];
    if (!m) return;
    setSelected(m);
    // Defer to next tick so the DOM has the highlighted row.
    queueMicrotask(() => {
      document
        .querySelector<HTMLElement>(`[data-memory-row="${m.id}"]`)
        ?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    });
  }, [cursorIndex, memories]);

  /**
   * Run a single-id mutation across every selected memory with bounded
   * concurrency, then report partial-success in a single toast and invalidate
   * the affected caches once.
   */
  const runBulk = useCallback(
    async (
      verb: (id: string) => Promise<unknown>,
      successKey: 'bulk.promotedToast' | 'bulk.demotedToast' | 'bulk.deletedToast',
    ) => {
      const ids = Array.from(multi.selected);
      if (ids.length === 0) return;
      setBulkBusy(true);
      try {
        const result = await runWithConcurrency(ids, verb, 5);
        const okCount = result.succeeded.length;
        const failCount = result.failed.length;
        if (failCount === 0) {
          toast(t(successKey, { count: okCount }), 'success');
        } else if (okCount === 0) {
          const first = result.failed[0]?.error;
          const detail = first instanceof Error ? first.message : t('common.error');
          toast(t('bulk.allFailedToast', { count: failCount, detail }), 'error');
        } else {
          toast(t('bulk.partialFailureToast', { ok: okCount, failed: failCount }), 'error');
        }
        multi.clear();
        invalidate();
      } finally {
        setBulkBusy(false);
      }
    },
    [multi, invalidate, t],
  );

  const onBulkPromote = () => runBulk((id) => api.memories.promote(id), 'bulk.promotedToast');
  const onBulkDemote = () => runBulk((id) => api.memories.demote(id), 'bulk.demotedToast');

  // Deferred bulk delete with single-toast undo. Mirrors the single-row
  // delete pattern (snapshot → optimistic strip → 5s window → commit or
  // restore) but at the granularity of the entire selection. One toast for
  // N items keeps the screen calm and the undo affordance obvious.
  const BULK_UNDO_WINDOW_MS = 5000;
  const pendingBulkRef = useRef<{ timer: ReturnType<typeof setTimeout>; commit: () => void } | null>(null);

  // Flush any pending bulk delete on unmount so the user's intent isn't
  // silently dropped if they navigate away.
  useEffect(() => {
    return () => {
      pendingBulkRef.current?.commit();
    };
  }, []);

  const onBulkDelete = async () => {
    if (!multi.hasSelection) return;
    // Themed, focusable, testable alertdialog — see `stores/confirm.ts`.
    // We default focus onto the cancel button because this action is
    // destructive and we don't want hasty Enter presses to wipe data.
    const ok = await confirm({
      message: t('bulk.deleteConfirm', { count: multi.count }),
      destructive: true,
      confirmLabel: t('common.delete'),
    });
    if (!ok) return;

    // Commit any previous in-flight bulk delete first so we never have two
    // overlapping undo windows.
    pendingBulkRef.current?.commit();
    pendingBulkRef.current = null;

    const ids = Array.from(multi.selected);
    const idSet = new Set(ids);
    const count = ids.length;

    // Snapshot every cached memories list so we can roll back on undo.
    const snapshots: Array<[QueryKey, unknown]> = [];
    for (const q of qc.getQueryCache().findAll({ queryKey: ['memories'] })) {
      snapshots.push([q.queryKey, q.state.data]);
    }

    // Optimistically strip from cached lists. The helper knows the real
    // `{ total, memories }` envelope and leaves any other cache entry alone.
    qc.setQueriesData<unknown>({ queryKey: ['memories'] }, (old: unknown) => withoutMemories(old, idSet));

    // Close detail panel if its memory was in the selection.
    if (selected && idSet.has(selected.id)) setSelected(null);
    multi.clear();
    track(EVENT.memory_delete, { count });
    setBulkBusy(true);

    let committed = false;
    let cancelled = false;
    const restore = () => {
      for (const [key, data] of snapshots) qc.setQueryData(key, data);
    };
    const commit = () => {
      if (committed || cancelled) return;
      committed = true;
      clearTimeout(pendingBulkRef.current?.timer as ReturnType<typeof setTimeout>);
      pendingBulkRef.current = null;
      runWithConcurrency(ids, (id) => api.memories.delete(id), 5)
        .then((result) => {
          const okCount = result.succeeded.length;
          const failCount = result.failed.length;
          if (failCount === 0) {
            // Success is already implicit in the optimistic UI — emit a
            // muted toast only if anything was unexpected.
          } else if (okCount === 0) {
            const first = result.failed[0]?.error;
            const detail = first instanceof Error ? first.message : t('common.error');
            toast(t('bulk.allFailedToast', { count: failCount, detail }), 'error');
            restore();
          } else {
            toast(t('bulk.partialFailureToast', { ok: okCount, failed: failCount }), 'error');
          }
          invalidate();
        })
        .finally(() => setBulkBusy(false));
    };
    const timer = setTimeout(commit, BULK_UNDO_WINDOW_MS);
    pendingBulkRef.current = { timer, commit };

    toast(t('bulk.deletedPending', { count }), 'success', {
      duration: BULK_UNDO_WINDOW_MS,
      action: {
        label: t('common.undo'),
        onAction: () => {
          if (committed) return;
          cancelled = true;
          track(EVENT.memory_delete_undo, { count });
          clearTimeout(timer);
          pendingBulkRef.current = null;
          restore();
          setBulkBusy(false);
          toast(t('bulk.deleteUndone', { count }), 'info', { duration: 2500 });
        },
      },
    });
  };

  return (
    <div className="flex h-full relative">
      {/* Page anchor for screen readers. The visual page chrome is the
          two-column layout below; sr-only h1 keeps the document outline
          intact without competing for screen real estate. */}
      <h1 className="sr-only">{t('memories.title')}</h1>
      <div className="flex-1 flex flex-col min-w-0 border-r border-border">
        <div className="p-4 space-y-3 border-b border-border">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              handleSearch();
            }}
            className="flex gap-2"
          >
            <SearchInput
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                if (!e.target.value) setActiveQuery('');
              }}
              placeholder={t('memories.searchPlaceholder')}
              aria-label={t('memories.searchPlaceholder')}
              onSubmit={handleSearch}
            />
            <Button type="submit" variant="secondary">
              {t('common.search')}
            </Button>
          </form>
          {/* Recents only render when the input is empty so we don't fight
              with the autocomplete-style search results. Once the user
              starts typing, this row disappears to keep the page calm. */}
          {!query && recents.length > 0 && (
            <div className="flex items-center gap-1.5 flex-wrap text-xs">
              <span className="text-muted-foreground text-[11px] uppercase tracking-wider">{t('memories.recent')}</span>
              {recents.map((q) => (
                <button
                  key={q}
                  type="button"
                  onClick={() => runRecent(q)}
                  className="px-2 py-0.5 rounded-md bg-muted hover:bg-accent border border-border text-foreground transition-colors max-w-[180px] truncate"
                  title={q}
                >
                  {q}
                </button>
              ))}
              <button
                type="button"
                onClick={clearRecents}
                className="text-muted-foreground hover:text-foreground text-[11px] ml-1"
                aria-label={t('memories.recentClearAria')}
              >
                {t('common.clear')}
              </button>
            </div>
          )}

          <div className="flex gap-2 flex-wrap items-center">
            <label className="sr-only" htmlFor="type-filter">
              {t('memories.filterByType')}
            </label>
            <NativeSelect
              id="type-filter"
              value={typeFilter}
              onChange={(e) => setTypeFilter(e.target.value)}
              className="text-xs"
            >
              <option value="">{t('memories.filterByType')}</option>
              {Object.keys(NODE_TYPE_COLORS).map((type) => (
                <option key={type} value={type}>
                  {t(`nodeTypes.${type}`, { defaultValue: type })}
                </option>
              ))}
            </NativeSelect>
            <SearchInput
              value={tagFilter}
              onChange={(e) => setTagFilter(e.target.value)}
              placeholder={t('memories.filterByTag')}
              aria-label={t('memories.filterByTag')}
              className="!py-1 !px-2 !text-xs max-w-[160px]"
            />
            <span className="text-xs text-muted-foreground">{t('common.total', { count: total })}</span>
            {memories.length > 0 && (
              // biome-ignore lint/a11y/noLabelWithoutControl: <Checkbox/> renders a real <input type="checkbox"> that the label implicitly associates with
              <label className="ml-auto flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer select-none">
                <Checkbox
                  checked={multi.allSelected}
                  indeterminate={multi.partiallySelected}
                  onChange={() => (multi.allSelected ? multi.clear() : multi.selectAll())}
                  aria-label={t('bulk.selectAllAria')}
                />
                <span>{t('bulk.selectAll')}</span>
              </label>
            )}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-2 space-y-1" aria-busy={loading}>
          {isError && <QueryErrorPanel className="m-2" error={queryError} onRetry={retry} />}
          {loading && memories.length === 0 && !isError && <SkeletonList count={6} cardProps={{ lines: 2 }} />}
          {!loading && !isError && memories.length === 0 && (
            <EmptyState
              icon="◌"
              title={t('memories.noMemories')}
              description={t('memories.noMemoriesHint')}
              action={
                <Button
                  type="button"
                  variant="default"
                  size="sm"
                  // Same path the ⌘N shortcut and the FAB use — Layout
                  // owns the dialog state via `useDialogStore`.
                  onClick={() => useDialogStore.getState().openAddMemory('event')}
                >
                  {t('memories.emptyCta')}
                </Button>
              }
            />
          )}
          {memories.map((m, idx) => (
            <MemoryListItem
              key={m.id}
              memory={m}
              isActive={selected?.id === m.id}
              isSelected={multi.isSelected(m.id)}
              selectionMode={multi.hasSelection}
              isFocused={cursorIndex === idx}
              isPinned={pinned.has(m.id)}
              onActivate={setSelected}
              onToggleSelect={multi.toggle}
            />
          ))}
          {/* Pagination — only shown when list mode is active (no active
              search) and there's more than one page worth of data. Search
              results come back as a flat top-K so they don't paginate. */}
          {!activeQuery && total > PAGE_SIZE && (
            <nav
              className="flex items-center justify-between gap-2 pt-2 px-2 text-xs text-muted-foreground"
              aria-label={t('memories.pagination')}
            >
              <span>
                {t('memories.pageRange', {
                  from: total === 0 ? 0 : pageOffset + 1,
                  to: Math.min(total, pageOffset + memories.length),
                  total,
                })}
              </span>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={pageOffset === 0 || loading}
                  onClick={() => setPageOffset((o) => Math.max(0, o - PAGE_SIZE))}
                >
                  {t('common.previous')}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={pageOffset + memories.length >= total || loading}
                  onClick={() => setPageOffset((o) => o + PAGE_SIZE)}
                >
                  {t('common.next')}
                </Button>
              </div>
            </nav>
          )}
        </div>
      </div>

      <div className="hidden lg:flex flex-col w-96 p-4 overflow-y-auto">
        {selected ? (
          <MemoryDetail memory={selected} onUpdate={invalidate} onClose={() => setSelected(null)} />
        ) : (
          <EmptyState icon="◉" title={t('memories.selectMemory')} className="h-full" />
        )}
      </div>

      <BulkActionBar
        count={multi.count}
        busy={bulkBusy}
        onPromote={onBulkPromote}
        onDemote={onBulkDemote}
        onDelete={onBulkDelete}
        onClear={multi.clear}
      />
    </div>
  );
}
