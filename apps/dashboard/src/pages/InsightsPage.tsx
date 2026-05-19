import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { RunDreamCycleButton } from '@/components/dream/RunDreamCycleButton';
import { InsightCard } from '@/components/InsightCard';
import { EmptyState } from '@/components/ui/empty-state';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SegmentedControl } from '@/components/ui/segmented-control';
import { SkeletonList } from '@/components/ui/skeleton';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useTrackPageView } from '@/stores/telemetry';

/**
 * Insights tab — Proposal B surface.
 *
 * Lists every node of type `"insight"` along with its structured
 * Proposal B metadata. Filter switches between `all | validated |
 * unvalidated`; the server applies the filter so we don't ship
 * irrelevant payloads to the client.
 *
 * Read-only by design (same rationale as the Decisions tab): insights
 * are written by the dream cycle / reflect tool and validated through
 * the `memory(action="promote")` MCP path. Editing one inline would
 * skip the validation pipeline and corrupt the `validatedByAgent`
 * audit trail.
 */
type FilterValue = 'all' | 'validated' | 'unvalidated';

export function InsightsPage() {
  useTrackPageView('insights');
  const { t } = useTranslation();
  const [filter, setFilter] = useState<FilterValue>('all');

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: queryKeys.insights(filter, 50),
    queryFn: () => api.insights(filter, 50),
  });

  const filterOptions = useMemo(
    () =>
      (['all', 'validated', 'unvalidated'] as const).map((value) => ({
        value,
        label: t(`insights.filter.${value}`, value),
      })),
    [t],
  );

  const insights = data?.insights ?? [];
  const validatedCount = data?.validatedCount ?? 0;

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h2 className="text-lg font-bold text-foreground">{t('insights.title', 'Insights')}</h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            {t(
              'insights.subtitleHeader',
              'Tentative observations from dream cycles and reflect runs. Promote to validate.',
            )}
          </p>
        </div>
        <SegmentedControl value={filter} onChange={(v) => setFilter(v as FilterValue)} options={filterOptions} />
      </div>

      {!isLoading && !isError && insights.length > 0 && (
        <div className="text-xs text-muted-foreground">
          {t('insights.summary', {
            total: insights.length,
            validated: validatedCount,
            defaultValue: '{{validated}} validated of {{total}} shown',
          })}
        </div>
      )}

      {isError ? (
        <QueryErrorPanel error={error} onRetry={refetch} />
      ) : isLoading ? (
        <SkeletonList count={3} />
      ) : insights.length === 0 ? (
        <EmptyState
          icon="◇"
          title={t('insights.empty.title', 'No insights yet')}
          description={t(
            'insights.empty.description',
            'Run a dream cycle to generate synthesised observations across your memories.',
          )}
          action={<RunDreamCycleButton labelKey="insights.empty.cta" size="sm" />}
        />
      ) : (
        <div className="space-y-3">
          {insights.map((insight) => (
            <InsightCard key={insight.id} insight={insight} />
          ))}
        </div>
      )}
    </div>
  );
}
