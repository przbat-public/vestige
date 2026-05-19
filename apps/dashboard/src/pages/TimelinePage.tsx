import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Card } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { ProgressBar } from '@/components/ui/progress-bar';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SegmentedControl } from '@/components/ui/segmented-control';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useTrackPageView } from '@/stores/telemetry';
import { NODE_TYPE_COLORS, retentionColor } from '@/types';

const DAY_OPTIONS = [7, 14, 30, 90] as const;
type DayOption = (typeof DAY_OPTIONS)[number];

export function TimelinePage() {
  useTrackPageView('timeline');
  const { t, i18n } = useTranslation();
  const [days, setDays] = useState<DayOption>(7);
  const {
    data,
    isLoading: loading,
    isError,
    error,
    refetch,
  } = useQuery({
    queryKey: queryKeys.timeline(days, 500),
    queryFn: () => api.timeline(days, 500),
  });
  const timeline = data?.timeline ?? [];
  const maxCount = Math.max(...timeline.map((d) => d.count), 1);

  const segmentOptions = useMemo(() => DAY_OPTIONS.map((d) => ({ value: String(d), label: `${d}d` })), []);

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-bold text-foreground">{t('timeline.title')}</h2>
        <SegmentedControl
          value={String(days)}
          onChange={(v) => setDays(Number(v) as DayOption)}
          options={segmentOptions}
        />
      </div>

      {isError ? (
        <QueryErrorPanel error={error} onRetry={refetch} />
      ) : loading ? (
        <LoadingSpinner label={t('common.loading')} />
      ) : timeline.length === 0 ? (
        <EmptyState icon="◎" title={t('timeline.noActivity')} />
      ) : (
        <div className="space-y-2">
          {timeline.map((day) => (
            <Card key={day.date} className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-xs text-foreground font-medium">
                  {new Date(day.date).toLocaleDateString(i18n.language, {
                    weekday: 'short',
                    month: 'short',
                    day: 'numeric',
                  })}
                </span>
                <span className="text-xs text-muted-foreground">
                  {t('timeline.memoriesOn', { count: day.count, date: '' }).replace(/ on $/, '')}
                </span>
              </div>
              <ProgressBar
                value={day.count}
                max={maxCount}
                label={day.date}
                color="var(--color-primary)"
                showValue={false}
              />
              {day.memories.length > 0 && (
                <div className="space-y-1 pt-1">
                  {day.memories.slice(0, 5).map((m) => (
                    <div key={m.id} className="flex items-center gap-2 text-xs min-w-0">
                      <span
                        className="w-1.5 h-1.5 rounded-full flex-shrink-0"
                        style={{ backgroundColor: NODE_TYPE_COLORS[m.nodeType] || '#8B95A5' }}
                        aria-hidden="true"
                      />
                      <span className="text-muted-foreground truncate">{m.content.slice(0, 60)}</span>
                      <span
                        className="ml-auto flex-shrink-0 tabular-nums"
                        style={{ color: retentionColor(m.retentionStrength) }}
                      >
                        {(m.retentionStrength * 100).toFixed(0)}%
                      </span>
                    </div>
                  ))}
                  {day.memories.length > 5 && (
                    <div className="text-xs text-muted-foreground">
                      {t('timeline.moreCount', { count: day.memories.length - 5 })}
                    </div>
                  )}
                </div>
              )}
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
