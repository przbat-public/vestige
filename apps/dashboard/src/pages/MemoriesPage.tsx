import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MemoryListItem } from '@/components/memories/MemoryListItem';
import { MemoryDetail } from '@/components/memories/MemoryDetail';
import { SearchInput } from '@/components/ui/search-input';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import type { Memory } from '@/types';
import { NODE_TYPE_COLORS } from '@/types';

export function MemoriesPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [query, setQuery] = useState('');
  const [activeQuery, setActiveQuery] = useState('');
  const [selected, setSelected] = useState<Memory | null>(null);
  const [typeFilter, setTypeFilter] = useState('');
  const [tagFilter, setTagFilter] = useState('');

  const listParams = { limit: '100', ...(typeFilter && { type: typeFilter }), ...(tagFilter && { tag: tagFilter }) };

  const { data: listData, isLoading: listLoading } = useQuery({
    queryKey: queryKeys.memories(listParams),
    queryFn: () => api.memories.list(listParams),
    enabled: !activeQuery,
  });

  const { data: searchData, isLoading: searchLoading } = useQuery({
    queryKey: queryKeys.search(activeQuery, 50),
    queryFn: () => api.search(activeQuery, 50),
    enabled: !!activeQuery,
  });

  const memories = activeQuery ? (searchData?.results ?? []) : (listData?.memories ?? []);
  const total = activeQuery ? (searchData?.total ?? 0) : (listData?.total ?? 0);
  const loading = activeQuery ? searchLoading : listLoading;

  const handleSearch = () => setActiveQuery(query);
  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ['memories'] });
    qc.invalidateQueries({ queryKey: ['search'] });
  };

  return (
    <div className="flex h-full">
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
            <button
              type="submit"
              className="px-4 py-2 rounded-xl text-sm bg-primary/10 text-primary hover:bg-primary/20 transition"
            >
              {t('common.search')}
            </button>
          </form>

          <div className="flex gap-2 flex-wrap items-center">
            <label className="sr-only" htmlFor="type-filter">{t('memories.filterByType')}</label>
            <select
              id="type-filter"
              value={typeFilter}
              onChange={(e) => setTypeFilter(e.target.value)}
              className="px-2 py-1 rounded-lg text-xs text-muted-foreground bg-background border border-border focus:outline-none focus:ring-2 focus:ring-ring"
            >
              <option value="">{t('memories.filterByType')}</option>
              {Object.keys(NODE_TYPE_COLORS).map((type) => (
                <option key={type} value={type}>
                  {t(`nodeTypes.${type}`, { defaultValue: type })}
                </option>
              ))}
            </select>
            <SearchInput
              value={tagFilter}
              onChange={(e) => setTagFilter(e.target.value)}
              placeholder={t('memories.filterByTag')}
              aria-label={t('memories.filterByTag')}
              className="!py-1 !px-2 !text-xs max-w-[160px]"
            />
            <span className="text-xs text-muted-foreground">{t('common.total', { count: total })}</span>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-2 space-y-1" aria-busy={loading}>
          {loading && memories.length === 0 && (
            <LoadingSpinner label={t('common.loading')} />
          )}
          {!loading && memories.length === 0 && (
            <EmptyState icon="◌" title={t('memories.noMemories')} description={t('memories.noMemoriesHint')} />
          )}
          {memories.map((m) => (
            <MemoryListItem key={m.id} memory={m} isSelected={selected?.id === m.id} onSelect={setSelected} />
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
    </div>
  );
}
