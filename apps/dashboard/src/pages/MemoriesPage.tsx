import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MemoryDetail } from '@/components/memories/MemoryDetail';
import { MemoryListItem } from '@/components/memories/MemoryListItem';
import { BulkActionBar } from '@/components/memories/BulkActionBar';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { NativeSelect } from '@/components/ui/native-select';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SearchInput } from '@/components/ui/search-input';
import { useMultiSelect } from '@/hooks/use-multi-select';
import { runWithConcurrency } from '@/lib/concurrency';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { toast } from '@/stores/toast';
import type { Memory } from '@/types';
import { NODE_TYPE_COLORS } from '@/types';

const memoryId = (m: Memory): string => m.id;

export function MemoriesPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [query, setQuery] = useState('');
  const [activeQuery, setActiveQuery] = useState('');
  const [selected, setSelected] = useState<Memory | null>(null);
  const [typeFilter, setTypeFilter] = useState('');
  const [tagFilter, setTagFilter] = useState('');
  const [bulkBusy, setBulkBusy] = useState(false);

  const listParams = { limit: '100', ...(typeFilter && { type: typeFilter }), ...(tagFilter && { tag: tagFilter }) };

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

  const memories = activeQuery ? (searchData?.results ?? []) : (listData?.memories ?? []);
  const total = activeQuery ? (searchData?.total ?? 0) : (listData?.total ?? 0);
  const loading = activeQuery ? searchLoading : listLoading;
  const isError = activeQuery ? searchError : listError;
  const queryError = activeQuery ? searchErrorObj : listErrorObj;
  const retry = activeQuery ? refetchSearch : refetchList;

  const multi = useMultiSelect(memories, memoryId);

  const handleSearch = () => setActiveQuery(query);
  const invalidate = useCallback(() => {
    qc.invalidateQueries({ queryKey: ['memories'] });
    qc.invalidateQueries({ queryKey: ['search'] });
    qc.invalidateQueries({ queryKey: queryKeys.stats });
  }, [qc]);

  // Esc clears selection, Cmd/Ctrl+A selects all visible. We register on the
  // window so the shortcut works regardless of focus location, but ignore
  // events while typing — defined as TEXT inputs and textareas. Checkbox /
  // radio / button inputs are not considered "typing" so the shortcuts still
  // fire when focus is on a row checkbox (Linear/Notion behavior).
  useEffect(() => {
    const isTypingTarget = (el: HTMLElement | null): boolean => {
      if (!el) return false;
      if (el.isContentEditable) return true;
      if (el.tagName === 'TEXTAREA') return true;
      if (el.tagName === 'INPUT') {
        const type = (el as HTMLInputElement).type;
        // Allow shortcuts when focus is on non-text inputs (checkbox/radio/etc).
        return type !== 'checkbox' && type !== 'radio' && type !== 'button' && type !== 'submit';
      }
      return false;
    };
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing = isTypingTarget(target);
      if (e.key === 'Escape' && multi.hasSelection && !typing) {
        multi.clear();
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'a' && !typing) {
        e.preventDefault();
        multi.selectAll();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [multi]);

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
  const onBulkDelete = () => {
    if (!multi.hasSelection) return;
    const ok = window.confirm(t('bulk.deleteConfirm', { count: multi.count }));
    if (!ok) return;
    runBulk((id) => api.memories.delete(id), 'bulk.deletedToast');
    // Close detail panel if its memory was in the selection.
    if (selected && multi.selected.has(selected.id)) setSelected(null);
  };

  return (
    <div className="flex h-full relative">
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
          {loading && memories.length === 0 && !isError && <LoadingSpinner label={t('common.loading')} />}
          {!loading && !isError && memories.length === 0 && (
            <EmptyState icon="◌" title={t('memories.noMemories')} description={t('memories.noMemoriesHint')} />
          )}
          {memories.map((m) => (
            <MemoryListItem
              key={m.id}
              memory={m}
              isActive={selected?.id === m.id}
              isSelected={multi.isSelected(m.id)}
              selectionMode={multi.hasSelection}
              onActivate={setSelected}
              onToggleSelect={multi.toggle}
            />
          ))}
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
