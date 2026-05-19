import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DecisionCard } from '@/components/DecisionCard';
import { EmptyState } from '@/components/ui/empty-state';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SegmentedControl } from '@/components/ui/segmented-control';
import { SkeletonList } from '@/components/ui/skeleton';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useTrackPageView } from '@/stores/telemetry';

/**
 * Decisions tab — comparison-matrix view of every
 * `remember_decision_v2` payload stored in the graph.
 *
 * Read-only by design. Decisions are written through the MCP tool (the
 * agent is the author), so the dashboard only surfaces them; editing a
 * decision from the UI would let the user diverge from the canonical
 * memory content + extra_json payload pair and silently break stale
 * detection. If we ever add edit, it has to round-trip through the
 * tool's validation.
 *
 * Filter `all | active | expired` is client-side over the same query:
 *   * `active`  → `expired === false`
 *   * `expired` → `expired === true`
 * The server returns both because either count is informative even when
 * the user is looking at the opposite tab, and the dataset is bounded
 * (50 latest decisions per request).
 */
type FilterValue = 'all' | 'active' | 'expired';

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: list page composes a filter dropdown, an empty-state branch, a loading branch, an error branch, and a per-row TimelineDot decorator — splitting any one would force shared state up into props and obscure flow
export function DecisionsPage() {
  useTrackPageView('decisions');
  const { t } = useTranslation();
  const [filter, setFilter] = useState<FilterValue>('all');

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: queryKeys.decisions(50),
    queryFn: () => api.decisions(50),
  });

  const filterOptions = useMemo(
    () =>
      (['all', 'active', 'expired'] as const).map((value) => ({
        value,
        label: t(`decisions.filter.${value}`),
      })),
    [t],
  );

  const decisions = data?.decisions ?? [];
  const filtered = decisions.filter((d) => {
    if (filter === 'active') return !d.expired;
    if (filter === 'expired') return d.expired;
    return true;
  });

  const activeCount = decisions.filter((d) => !d.expired).length;
  const expiredCount = decisions.filter((d) => d.expired).length;

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h2 className="text-lg font-bold text-foreground">{t('decisions.title')}</h2>
          <p className="text-xs text-muted-foreground mt-0.5">{t('decisions.subtitleHeader')}</p>
        </div>
        <SegmentedControl value={filter} onChange={(v) => setFilter(v as FilterValue)} options={filterOptions} />
      </div>

      {!isLoading && !isError && decisions.length > 0 && (
        <div className="text-xs text-muted-foreground">
          {t('decisions.summary', {
            total: decisions.length,
            active: activeCount,
            expired: expiredCount,
          })}
        </div>
      )}

      {isError ? (
        <QueryErrorPanel error={error} onRetry={refetch} />
      ) : isLoading ? (
        <SkeletonList count={3} />
      ) : filtered.length === 0 ? (
        <EmptyState
          icon="◇"
          title={decisions.length === 0 ? t('decisions.empty.title') : t(`decisions.empty.filteredTitle.${filter}`)}
          description={
            decisions.length === 0 ? t('decisions.empty.description') : t('decisions.empty.filteredDescription')
          }
        />
      ) : (
        <div className="space-y-3">
          {filtered.map((decision) => (
            <DecisionCard key={decision.id} decision={decision} />
          ))}
        </div>
      )}
    </div>
  );
}
