import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Card } from '@/components/ui/card';
import { ProgressBar } from '@/components/ui/progress-bar';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { NODE_TYPE_COLORS, retentionColor } from '@/types';

export function TimelinePage() {
  const { t, i18n } = useTranslation();
  const [days, setDays] = useState(7);
  const { data, isLoading: loading } = useQuery({
    queryKey: queryKeys.timeline(days, 500),
    queryFn: () => api.timeline(days, 500),
  });
  const timeline = data?.timeline ?? [];
  const maxCount = Math.max(...timeline.map((d) => d.count), 1);

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-bold text-foreground">{t('timeline.title')}</h2>
        <div className="flex gap-1">
          {[7, 14, 30, 90].map((d) => (
            <button
              type="button"
              key={d}
              onClick={() => setDays(d)}
              className={`px-3 py-1 rounded-lg text-xs transition ${
                days === d ? 'bg-primary/15 text-primary font-medium' : 'text-muted-foreground hover:text-foreground hover:bg-accent'
              }`}
              aria-pressed={days === d}
            >
              {d}d
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <LoadingSpinner label={t('common.loading')} />
      ) : timeline.length === 0 ? (
        <EmptyState icon="◎" title={t('timeline.noActivity')} />
      ) : (
        <div className="space-y-2">
          {timeline.map((day) => (
            <Card key={day.date} className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-xs text-foreground font-medium">
                  {new Date(day.date).toLocaleDateString(i18n.language, { weekday: 'short', month: 'short', day: 'numeric' })}
                </span>
                <span className="text-xs text-muted-foreground">
                  {t('timeline.memoriesOn', { count: day.count, date: '' }).replace(/ on $/, '')}
                </span>
              </div>
              <ProgressBar value={day.count} max={maxCount} label={day.date} color="var(--color-primary)" showValue={false} />
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
                      <span className="ml-auto flex-shrink-0 tabular-nums" style={{ color: retentionColor(m.retentionStrength) }}>
                        {(m.retentionStrength * 100).toFixed(0)}%
                      </span>
                    </div>
                  ))}
                  {day.memories.length > 5 && (
                    <div className="text-xs text-muted-foreground">+{day.memories.length - 5} more</div>
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
