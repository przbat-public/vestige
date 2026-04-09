import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Alert } from '@/components/ui/alert';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { ProgressBar } from '@/components/ui/progress-bar';
import { StatCard } from '@/components/ui/stat-card';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { NODE_TYPE_COLORS, retentionColor } from '@/types';

export function StatsPage() {
  const { t } = useTranslation();
  const { data: stats, isError: statsError } = useQuery({ queryKey: queryKeys.stats, queryFn: api.stats });
  const { data: health, isError: healthError } = useQuery({ queryKey: queryKeys.health, queryFn: api.health });
  const { data: distribution } = useQuery({
    queryKey: queryKeys.retentionDistribution,
    queryFn: api.retentionDistribution,
  });

  const healthColor: Record<string, string> = {
    healthy: '#10b981',
    degraded: '#f59e0b',
    critical: '#ef4444',
    empty: '#6b7280',
  };

  if (statsError && healthError) return <Alert variant="destructive" className="m-4">{t('common.fetchError')}</Alert>;
  if (!stats && !health) return <LoadingSpinner label={t('common.loading')} className="h-full" />;

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <h2 className="text-lg font-bold text-foreground">{t('stats.title')}</h2>

      {health && (
        <Card>
          <div className="flex items-center gap-3">
            <span
              className="w-3 h-3 rounded-full"
              style={{ backgroundColor: healthColor[health.status] || '#6b7280' }}
            />
            <span className="text-sm font-medium text-foreground capitalize">{health.status}</span>
            <span className="text-xs text-muted-foreground">v{health.version}</span>
          </div>
        </Card>
      )}

      {stats && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          <StatCard label={t('stats.totalMemories')} value={stats.totalMemories} />
          <StatCard label={t('stats.dueForReview')} value={stats.dueForReview} />
          <StatCard
            label={t('stats.avgRetention')}
            value={`${(stats.averageRetention * 100).toFixed(1)}%`}
            color={retentionColor(stats.averageRetention)}
          />
          <StatCard label={t('stats.embeddingCoverage')} value={`${stats.embeddingCoverage.toFixed(0)}%`} />
        </div>
      )}

      {distribution && (
        <Card>
          <CardHeader>
            <CardTitle>{t('stats.retentionDistribution')}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1.5">
            {(() => {
              const maxCount = Math.max(...distribution.distribution.map((d) => d.count), 1);
              return distribution.distribution.map((bucket) => (
                <div key={bucket.range} className="flex items-center gap-2 text-xs">
                  <span className="w-16 text-muted-foreground text-right tabular-nums">{bucket.range}</span>
                  <ProgressBar
                    value={bucket.count}
                    max={maxCount}
                    label={bucket.range}
                    color="var(--color-primary)"
                    className="flex-1"
                    showValue={false}
                  />
                  <span className="w-8 text-foreground text-right tabular-nums">{bucket.count}</span>
                </div>
              ));
            })()}
          </CardContent>
        </Card>
      )}

      {distribution?.endangered && distribution.endangered.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-destructive">{t('stats.endangered')}</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-xs text-muted-foreground mb-2">{t('stats.endangeredHint')}</p>
            <div className="space-y-1">
              {distribution.endangered.slice(0, 5).map((m) => (
                <div key={m.id} className="text-xs text-muted-foreground truncate">
                  {m.content.slice(0, 60)} —{' '}
                  <span style={{ color: retentionColor(m.retentionStrength) }}>
                    {(m.retentionStrength * 100).toFixed(0)}%
                  </span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {distribution?.byType && (
        <Card>
          <CardHeader>
            <CardTitle>{t('stats.byType')}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-xs">
              {Object.entries(distribution.byType).map(([type, count]) => (
                <div key={type} className="flex items-center gap-2">
                  <span
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: NODE_TYPE_COLORS[type] || '#8B95A5' }}
                  />
                  <span className="text-muted-foreground">{t(`nodeTypes.${type}`, { defaultValue: type })}</span>
                  <span className="text-foreground font-medium tabular-nums ml-auto">{count}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
