import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { useDashboardLimits } from '@/hooks/use-dashboard-limits';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';

interface MemoryRevisionsPanelProps {
  memoryId: string;
}

/**
 * Collapsible *content* history for a single memory (`memory_revisions`, V17).
 *
 * This is not the changelog. The changelog lists life-cycle state transitions
 * ("active → dormant, reason: decay"); this lists what the memory used to
 * *say*. The dashboard previously titled the state-transition panel "Edit
 * history", which promised exactly this view and delivered the other one —
 * hence two panels, each labelled for what it shows.
 *
 * Page size comes from `GET /api/_meta/limits` via `useDashboardLimits()`, in
 * line with the repository's rule against component-local limits.
 *
 * Lazy-loads on open, and uses native `<details>` for keyboard accessibility
 * (Tab + Space/Enter to toggle, announced as a disclosure widget).
 */
export function MemoryRevisionsPanel({ memoryId }: MemoryRevisionsPanelProps) {
  const { t, i18n } = useTranslation();
  const limits = useDashboardLimits();
  const [opened, setOpened] = useState(false);

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: queryKeys.memoryRevisions(memoryId, limits.revisionHistoryLimitDefault),
    queryFn: () => api.memories.revisions(memoryId, limits.revisionHistoryLimitDefault),
    enabled: opened,
    staleTime: 60_000,
  });

  const formatter = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        dateStyle: 'short',
        timeStyle: 'short',
      }),
    [i18n.language],
  );

  return (
    <details
      className="rounded-lg border border-border/60 bg-card/50"
      onToggle={(e) => setOpened((e.currentTarget as HTMLDetailsElement).open)}
    >
      <summary className="cursor-pointer select-none px-3 py-2 text-xs font-medium text-muted-foreground hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg">
        {t('memories.revisions.title')}
        {data?.total ? <span className="ml-2 text-[10px] tabular-nums opacity-70">({data.total})</span> : null}
      </summary>
      <div className="px-3 pb-3 pt-1 space-y-2">
        {isLoading && <LoadingSpinner label={t('common.loading')} />}
        {isError && <QueryErrorPanel error={error} onRetry={refetch} />}
        {data && data.revisions.length === 0 && (
          <p className="text-xs text-muted-foreground italic">{t('memories.revisions.empty')}</p>
        )}
        {data && data.revisions.length > 0 && (
          <ol className="space-y-2 text-xs">
            {data.revisions.map((revision) => (
              <li key={revision.id} className="flex flex-col gap-0.5 border-l-2 border-border pl-3 py-1">
                <div className="flex flex-wrap items-center gap-2 text-muted-foreground tabular-nums">
                  <time dateTime={revision.recordedAt}>{formatter.format(new Date(revision.recordedAt))}</time>
                  <Badge variant="secondary">
                    {t(`memories.revisions.kind.${revision.kind}`, { defaultValue: revision.kind })}
                  </Badge>
                  {revision.actor && (
                    <span className="opacity-70">{t('memories.revisions.actor', { actor: revision.actor })}</span>
                  )}
                </div>
                {revision.reason && <div className="text-muted-foreground italic break-words">{revision.reason}</div>}
                {revision.oldContent && (
                  <div className="min-w-0">
                    <span className="text-muted-foreground">{t('memories.revisions.before')}: </span>
                    <span className="text-foreground/70 line-through break-words">{revision.oldContent}</span>
                  </div>
                )}
                {revision.newContent && (
                  <div className="min-w-0">
                    <span className="text-muted-foreground">{t('memories.revisions.after')}: </span>
                    <span className="text-foreground/90 break-words whitespace-pre-wrap">{revision.newContent}</span>
                  </div>
                )}
              </li>
            ))}
          </ol>
        )}
      </div>
    </details>
  );
}
