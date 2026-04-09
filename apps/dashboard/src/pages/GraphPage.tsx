import { useQuery } from '@tanstack/react-query';
import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Graph3D } from '@/components/Graph3D';
import { MemoryDetail } from '@/components/memories/MemoryDetail';
import { SectionErrorBoundary } from '@/components/SectionErrorBoundary';
import { TimeSlider } from '@/components/TimeSlider';
import { Alert } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { NativeSelect } from '@/components/ui/native-select';
import { SearchInput } from '@/components/ui/search-input';
import { Sheet } from '@/components/ui/sheet';
import { filterByDate } from '@/graph/temporal';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useWebSocket } from '@/stores/websocket';
import type { GraphEdge, GraphNode } from '@/types';

export function GraphPage() {
  const { t } = useTranslation();
  const events = useWebSocket((s) => s.events);

  const [searchQuery, setSearchQuery] = useState('');
  const [activeQuery, setActiveQuery] = useState<string | undefined>();
  const [maxNodes, setMaxNodes] = useState(150);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [temporalEnabled, setTemporalEnabled] = useState(false);
  const [temporalDate, setTemporalDate] = useState<Date | null>(null);

  const graphParams = { max_nodes: maxNodes, depth: 2, query: activeQuery };
  const { data: graphData, isLoading, isError } = useQuery({
    queryKey: queryKeys.graph(graphParams),
    queryFn: () => api.graph(graphParams),
  });

  const { data: selectedMemory } = useQuery({
    queryKey: queryKeys.memory(selectedNode ?? ''),
    queryFn: () => api.memories.get(selectedNode as string),
    enabled: !!selectedNode,
  });

  const temporalState = useMemo(() => {
    if (!graphData || !temporalEnabled || !temporalDate) return null;
    return filterByDate(graphData.nodes, graphData.edges, temporalDate);
  }, [graphData, temporalEnabled, temporalDate]);

  const displayNodes: GraphNode[] = temporalState?.visibleNodes ?? graphData?.nodes ?? [];
  const displayEdges: GraphEdge[] = temporalState?.visibleEdges ?? graphData?.edges ?? [];
  const nodeOpacities = temporalState?.nodeOpacities ?? new Map<string, number>();

  const onSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setActiveQuery(searchQuery || undefined);
  };

  const onNodeSelect = useCallback((nodeId: string) => {
    setSelectedNode(nodeId);
  }, []);

  const onDateChange = useCallback((date: Date) => {
    setTemporalDate(date);
  }, []);

  const onTemporalToggle = useCallback((enabled: boolean) => {
    setTemporalEnabled(enabled);
    if (!enabled) setTemporalDate(null);
  }, []);

  const closeDetail = useCallback(() => {
    setSelectedNode(null);
  }, []);

  if (isLoading && !graphData) {
    return <LoadingSpinner label={t('graph.loadingGraph')} className="h-full" />;
  }

  if (isError) {
    return <Alert variant="destructive" className="m-4">{t('common.fetchError')}</Alert>;
  }

  if (!graphData || graphData.nodes.length === 0) {
    return <EmptyState icon="◈" title={t('graph.emptyGraph')} description={t('graph.emptyHint')} className="h-full" />;
  }

  return (
    <div className="h-full flex flex-col relative">
      <div className="absolute top-4 left-4 right-4 z-10 flex gap-2 items-center">
        <form onSubmit={onSearch} className="flex-1 flex gap-2">
          <SearchInput
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t('graph.searchPlaceholder')}
            aria-label={t('graph.searchPlaceholder')}
            onSubmit={() => setActiveQuery(searchQuery || undefined)}
            className="shadow-md"
          />
          <Button type="submit" variant="secondary" className="shadow-md">
            {t('common.search')}
          </Button>
        </form>
        <NativeSelect
          value={maxNodes}
          onChange={(e) => setMaxNodes(Number(e.target.value))}
          aria-label={t('graph.maxNodes')}
          className="text-xs shadow-md bg-card/80 backdrop-blur-sm"
        >
          <option value={50}>50 nodes</option>
          <option value={100}>100 nodes</option>
          <option value={150}>150 nodes</option>
          <option value={200}>200 nodes</option>
          <option value={300}>300 nodes</option>
          <option value={500}>500 nodes</option>
        </NativeSelect>
      </div>

      <div className="absolute top-16 left-4 z-10 flex items-center gap-2 text-xs text-muted-foreground">
        <span>{t('graph.nodesCount', { nodes: displayNodes.length })}</span>
        <span>·</span>
        <span>{t('graph.edgesCount', { edges: displayEdges.length })}</span>
        {temporalEnabled && <Badge variant="warning">{t('graph.temporalActive')}</Badge>}
      </div>

      <SectionErrorBoundary fallbackTitle={t('graph.title')}>
        <div className="flex-1">
          <Graph3D
            nodes={displayNodes}
            edges={displayEdges}
            centerId={graphData.centerId}
            events={events}
            nodeOpacities={nodeOpacities}
            onSelect={onNodeSelect}
          />
        </div>
      </SectionErrorBoundary>

      <TimeSlider nodes={graphData.nodes} onDateChange={onDateChange} onToggle={onTemporalToggle} />

      <Sheet open={!!selectedMemory} side="right" size="md" className="p-4">
        {selectedMemory && (
          <>
            <div className="flex justify-between items-center mb-3">
              <h3 className="text-sm font-semibold text-foreground">{t('graph.selected')}</h3>
              <Button variant="ghost" size="sm" onClick={closeDetail} aria-label={t('common.close')}>
                ×
              </Button>
            </div>
            <MemoryDetail memory={selectedMemory} onUpdate={closeDetail} onClose={closeDetail} />
          </>
        )}
      </Sheet>
    </div>
  );
}
