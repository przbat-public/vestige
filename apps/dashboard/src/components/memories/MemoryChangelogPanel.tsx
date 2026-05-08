import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';

interface MemoryChangelogPanelProps {
  memoryId: string;
}

/**
 * Collapsible audit-trail viewer for a single memory.
 *
 * Lazy-loads the changelog only when the user opens the disclosure — keeps the
 * detail panel cheap when the user only wants to read content. Each transition
 * is rendered as a row with timestamp, from→to state, and the reason type.
 *
 * Uses native `<details>` for keyboard accessibility (Tab + Space/Enter to
 * toggle, announced as a disclosure widget by all major screen readers).
 */
export function MemoryChangelogPanel({ memoryId }: MemoryChangelogPanelProps) {
  const { t, i18n } = useTranslation();
  const [opened, setOpened] = useState(false);

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: queryKeys.memoryChangelog(memoryId),
    queryFn: () => api.memories.changelog(memoryId),
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
        {t('memories.changelog.title')}
        {data?.totalTransitions ? (
          <span className="ml-2 text-[10px] tabular-nums opacity-70">({data.totalTransitions})</span>
        ) : null}
      </summary>
      <div className="px-3 pb-3 pt-1 space-y-2">
        {isLoading && <LoadingSpinner label={t('common.loading')} />}
        {isError && <QueryErrorPanel error={error} onRetry={refetch} />}
        {data && data.transitions.length === 0 && (
          <p className="text-xs text-muted-foreground italic">
            {t('memories.changelog.empty')}
          </p>
        )}
        {data && data.transitions.length > 0 && (
          <ol className="space-y-1.5 text-xs">
            {data.transitions.map((entry, idx) => (
              <li
                key={`${entry.timestamp}-${idx}`}
                className="flex flex-col gap-0.5 border-l-2 border-border pl-3 py-1"
              >
                <div className="flex items-center gap-2 text-muted-foreground tabular-nums">
                  <time dateTime={entry.timestamp}>
                    {formatter.format(new Date(entry.timestamp))}
                  </time>
                  <span aria-hidden="true">·</span>
                  <span className="text-foreground font-medium">{entry.reasonType}</span>
                </div>
                <div className="text-foreground/80">
                  <span className="opacity-60">{entry.fromState}</span>
                  <span aria-label="changed to" className="mx-1.5">→</span>
                  <span>{entry.toState}</span>
                </div>
                {entry.reasonData && (
                  <div className="text-muted-foreground italic break-words">
                    {entry.reasonData}
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
