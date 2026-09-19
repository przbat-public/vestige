import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { RunDreamCycleButton } from '@/components/dream/RunDreamCycleButton';
import { HubCard } from '@/components/HubCard';
import { EmptyState } from '@/components/ui/empty-state';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SkeletonList } from '@/components/ui/skeleton';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useTrackPageView } from '@/stores/telemetry';

export function HubsPage() {
  useTrackPageView('hubs');
  const { t } = useTranslation();

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: queryKeys.hubs(50),
    queryFn: () => api.hubs(50),
  });

  const hubs = data?.hubs ?? [];

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h2 className="text-lg font-bold text-foreground">{t('hubs.title', 'Topic Hubs')}</h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            {t(
              'hubs.subtitleHeader',
              'Clusters of related memories the dream cycle has stitched into a single summary.',
            )}
          </p>
        </div>
      </div>

      {!isLoading && !isError && hubs.length > 0 && (
        <div className="text-xs text-muted-foreground">
          {t('hubs.summary', {
            count: hubs.length,
          })}
        </div>
      )}

      {isError ? (
        <QueryErrorPanel error={error} onRetry={refetch} />
      ) : isLoading ? (
        <SkeletonList count={3} />
      ) : hubs.length === 0 ? (
        <EmptyState
          icon="☉"
          title={t('hubs.empty.title', 'No hubs yet')}
          description={t(
            'hubs.empty.description',
            'Topic hubs appear once a dream cycle discovers clusters of 5+ related memories.',
          )}
          action={<RunDreamCycleButton labelKey="hubs.empty.cta" size="sm" />}
        />
      ) : (
        <div className="space-y-3">
          {hubs.map((hub) => (
            <HubCard key={hub.id} hub={hub} />
          ))}
        </div>
      )}
    </div>
  );
}
