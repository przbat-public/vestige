import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { StatCard } from '@/components/ui/stat-card';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { NODE_TYPE_COLORS, retentionColor } from '@/types';

export function StatsPage() {
  const { t } = useTranslation();
  const {
    data: stats,
    isError: statsError,
    error: statsErrorObj,
    refetch: refetchStats,
  } = useQuery({ queryKey: queryKeys.stats, queryFn: api.stats });
  const {
    data: health,
    isError: healthError,
    error: healthErrorObj,
    refetch: refetchHealth,
  } = useQuery({ queryKey: queryKeys.health, queryFn: api.health });
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

  if (statsError && healthError) {
    return (
      <QueryErrorPanel
        className="m-4"
        error={statsErrorObj ?? healthErrorObj}
        onRetry={() => {
          refetchStats();
          refetchHealth();
        }}
      />
    );
  }
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

      {stats && (
        <Card>
          <CardHeader>
            <CardTitle>{t('stats.forgettingCurveTitle')}</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-xs text-muted-foreground mb-3">{t('stats.forgettingCurveHint')}</p>
            <ForgettingCurve averageRetention={stats.averageRetention} storageStrength={stats.averageStorageStrength} />
          </CardContent>
        </Card>
      )}

      {distribution && <RetentionHeatmap buckets={distribution.distribution} total={distribution.total} />}

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

interface ForgettingCurveProps {
  averageRetention: number;
  storageStrength: number;
}

/**
 * Renders a 90-day Ebbinghaus-style forgetting curve based on the user's
 * average storage strength.
 *
 * Math: R(t) = exp(-t / S_days) where S_days is mapped from
 * storage_strength (a [0,1] FSRS-derived signal). At storage=1.0 we want
 * roughly a 90-day half-life; at 0.0 a 1-day half-life. Linear mapping
 * `S_days = max(1, storage * 90)` gives a curve that's interpretable
 * without lying — the actual FSRS-6 schedule is per-memory and depends
 * on review history, but the average storage strength is a reasonable
 * proxy for "how much your knowledge base sticks".
 *
 * Drawn as inline SVG with viewBox to scale; no chart library — keeps
 * the StatsPage chunk lean. Y axis is implicit (0% bottom, 100% top),
 * X axis spans 90 days.
 */
function ForgettingCurve({ averageRetention, storageStrength }: ForgettingCurveProps) {
  const { t } = useTranslation();

  const path = useMemo(() => {
    const sDays = Math.max(1, storageStrength * 90);
    const points: string[] = [];
    for (let day = 0; day <= 90; day += 2) {
      const r = Math.exp(-day / sDays);
      // SVG coords: x in [0, 360], y in [0, 120], y=0 is top so invert.
      const x = (day / 90) * 360;
      const y = 120 - r * 120;
      points.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    }
    return `M${points.join(' L')}`;
  }, [storageStrength]);

  // Mark "today" — average current retention. Useful sanity check: if
  // the curve at t=0 disagrees with the dot, the curve is lying about
  // your base.
  const todayY = 120 - averageRetention * 120;

  return (
    <div className="space-y-2">
      <svg viewBox="0 0 380 140" className="w-full h-32" role="img" aria-label={t('stats.forgettingCurveAria')}>
        <title>{t('stats.forgettingCurveAria')}</title>
        {/* Y-axis grid lines at 100%, 50%, 0% */}
        {[0, 0.5, 1].map((frac) => {
          const y = 120 - frac * 120;
          return (
            <g key={frac}>
              <line x1="20" y1={y} x2="380" y2={y} stroke="currentColor" strokeOpacity="0.1" strokeDasharray="2 2" />
              <text x="0" y={y + 3} className="fill-muted-foreground" fontSize="9">
                {Math.round(frac * 100)}%
              </text>
            </g>
          );
        })}
        {/* X-axis tick labels at 7d, 30d, 90d */}
        {[7, 30, 90].map((d) => {
          const x = 20 + (d / 90) * 360;
          return (
            <g key={d}>
              <line x1={x} y1="120" x2={x} y2="124" stroke="currentColor" strokeOpacity="0.3" />
              <text x={x} y="135" textAnchor="middle" className="fill-muted-foreground" fontSize="9">
                {d}d
              </text>
            </g>
          );
        })}
        {/* The curve, translated 20px right to make room for y labels */}
        <g transform="translate(20, 0)">
          <path d={path} fill="none" stroke="var(--color-primary)" strokeWidth="2" />
          <circle cx="0" cy={todayY} r="4" fill="var(--color-primary)" />
          <text x="6" y={todayY - 6} className="fill-foreground" fontSize="10">
            {t('stats.todayLabel', { value: (averageRetention * 100).toFixed(0) })}
          </text>
        </g>
      </svg>
    </div>
  );
}

interface RetentionHeatmapProps {
  buckets: { range: string; count: number }[];
  total: number;
}

/**
 * Heatmap variant of the retention distribution — each bucket is a
 * coloured cell with the count overlaid. The colour ramp goes from red
 * (low retention, "endangered") through amber to green (high retention,
 * "stable"). Reading the heatmap is a single glance: a left-heavy band
 * means lots of forgotten memories; right-heavy means a healthy base.
 *
 * We deliberately keep the older progress-bar version's information
 * (count per bucket, percentage) but trade the bar-chart layout for a
 * 2-row banner so the page reads as a dashboard summary rather than a
 * stack of bars.
 */
function RetentionHeatmap({ buckets, total }: RetentionHeatmapProps) {
  const { t } = useTranslation();

  // Buckets come in retention order (0-0.1, 0.1-0.2, ..., 0.9-1.0). We
  // colour each cell by its bucket's centre retention, not by count, so
  // the visual mapping is "retention range" not "popularity".
  const cells = buckets.map((b, i) => {
    const centre = (i + 0.5) / buckets.length;
    return { ...b, centre, color: retentionColor(centre) };
  });
  const maxCount = Math.max(...cells.map((c) => c.count), 1);

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('stats.retentionDistribution')}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        <div className="grid grid-cols-10 gap-0.5 rounded-lg overflow-hidden">
          {cells.map((c) => {
            // Opacity encodes count density on top of bucket colour.
            // Empty buckets render almost transparent, the busiest one
            // at full opacity. Keeps colour identity (which retention
            // band) and density (how many) in the same cell.
            const opacity = 0.2 + 0.8 * (c.count / maxCount);
            return (
              <div
                key={c.range}
                className="h-12 flex flex-col items-center justify-center text-[10px] font-medium tabular-nums text-foreground"
                style={{ backgroundColor: c.color, opacity }}
                title={t('stats.heatmapCellTitle', { range: c.range, count: c.count, total })}
              >
                <span>{c.count}</span>
              </div>
            );
          })}
        </div>
        <div className="flex justify-between text-[10px] text-muted-foreground tabular-nums px-0.5">
          <span>{t('stats.heatmapLow')}</span>
          <span>{t('stats.heatmapHigh')}</span>
        </div>
      </CardContent>
    </Card>
  );
}
