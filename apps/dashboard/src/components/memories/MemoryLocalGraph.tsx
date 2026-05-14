import { useQuery } from '@tanstack/react-query';
import { lazy, Suspense } from 'react';
import { useTranslation } from 'react-i18next';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { useReducedMotion } from '@/hooks/use-reduced-motion';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useWebSocket } from '@/stores/websocket';

// Three.js + the Graph3D wrapper weigh ~178 kB gzipped. Loading them
// synchronously here put them on the critical path of MemoryDetail (which
// is itself behind a route-level code split). Defer the import so the
// detail panel renders the metadata first and the mini-graph fades in.
//
// `Suspense` below uses a small spinner that matches the loading-state branch
// so the user doesn't see two different placeholders.
const Graph3D = lazy(() => import('@/components/Graph3D').then((m) => ({ default: m.Graph3D })));

interface MemoryLocalGraphProps {
  memoryId: string;
  /** Depth of the BFS expansion. 1 shows direct neighbours, 2 shows neighbours-of-neighbours. */
  depth?: 1 | 2;
  /** Cap on nodes returned by the backend. Keep small for the detail panel. */
  maxNodes?: number;
  /** CSS height; the panel is shallow so we default to 16rem. */
  className?: string;
}

/**
 * Mini 3D graph of a memory's immediate neighbourhood.
 *
 * Uses the same `Graph3D` component as the main page but with a small
 * `max_nodes` cap and `depth=1` so the user sees just the connections that
 * touch this memory. Reuses every existing affordance (TrackballControls
 * for full 6DOF rotation, dream visual mode, prefers-reduced-motion).
 *
 * The query is keyed by `(memoryId, depth, maxNodes)` so different depth
 * settings cache separately. The backend uses `most_connected_memory` as
 * a fallback if no center is provided — we always pass `center_id`.
 */
export function MemoryLocalGraph({ memoryId, depth = 1, maxNodes = 30, className = 'h-64' }: MemoryLocalGraphProps) {
  const { t } = useTranslation();
  const events = useWebSocket((s) => s.events);
  const isDreaming = useWebSocket((s) => s.isDreaming);
  const reducedMotion = useReducedMotion();

  const params = { center_id: memoryId, depth, max_nodes: maxNodes } as const;

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: [...queryKeys.memory(memoryId), 'local-graph', depth, maxNodes],
    queryFn: () => api.graph(params),
    staleTime: 30_000,
  });

  if (isLoading) {
    return (
      <div className={`flex items-center justify-center ${className}`}>
        <LoadingSpinner label={t('graph.loadingGraph')} />
      </div>
    );
  }

  if (isError) {
    return <QueryErrorPanel className="m-1" error={error} onRetry={refetch} />;
  }

  if (!data || data.nodes.length === 0) {
    return <p className="text-xs text-muted-foreground italic px-2 py-3">{t('memories.localGraph.empty')}</p>;
  }

  return (
    <div className={`relative rounded-lg overflow-hidden border border-border/60 ${className}`}>
      <Suspense
        fallback={
          <div className={`flex items-center justify-center ${className}`}>
            <LoadingSpinner label={t('graph.loadingGraph')} />
          </div>
        }
      >
        <Graph3D
          nodes={data.nodes}
          edges={data.edges}
          centerId={data.centerId}
          events={events}
          isDreaming={isDreaming}
          reducedMotion={reducedMotion}
        />
      </Suspense>
      <div className="absolute bottom-1 right-2 text-[10px] text-muted-foreground/80 tabular-nums pointer-events-none">
        {t('graph.nodesCount', { nodes: data.nodes.length })} · {t('graph.edgesCount', { edges: data.edges.length })}
      </div>
    </div>
  );
}
