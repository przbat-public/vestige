import { useQuery } from '@tanstack/react-query';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Graph3D } from '@/components/Graph3D';
import { MemoryDetail } from '@/components/memories/MemoryDetail';
import { TimeSlider } from '@/components/TimeSlider';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { SearchInput } from '@/components/ui/search-input';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { EmptyState } from '@/components/ui/empty-state';
import { filterByDate } from '@/graph/temporal';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useWebSocket } from '@/stores/websocket';
import type { GraphEdge, GraphNode, Memory } from '@/types';

export function GraphPage() {
  const { t } = useTranslation();
  const events = useWebSocket((s) => s.events);

  const [searchQuery, setSearchQuery] = useState('');
  const [activeQuery, setActiveQuery] = useState<string | undefined>();
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [temporalEnabled, setTemporalEnabled] = useState(false);
  const [filteredNodes, setFilteredNodes] = useState<GraphNode[]>([]);
  const [filteredEdges, setFilteredEdges] = useState<GraphEdge[]>([]);
  const [nodeOpacities, setNodeOpacities] = useState<Map<string, number>>(new Map());

  const graphParams = { max_nodes: 100, depth: 2, query: activeQuery };
  const { data: graphData, isLoading } = useQuery({
    queryKey: queryKeys.graph(graphParams),
    queryFn: () => api.graph(graphParams),
    select: (data) => {
      if (filteredNodes.length === 0 && !temporalEnabled) {
        setFilteredNodes(data.nodes);
        setFilteredEdges(data.edges);
      }
      return data;
    },
  });

  const { data: selectedMemory } = useQuery({
    queryKey: queryKeys.memory(selectedNode ?? ''),
    queryFn: () => api.memories.get(selectedNode!),
    enabled: !!selectedNode,
  });

  const onSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setActiveQuery(searchQuery || undefined);
  };

  const onNodeSelect = useCallback((nodeId: string) => {
    setSelectedNode(nodeId);
  }, []);

  const onDateChange = useCallback(
    (date: Date) => {
      if (!graphData) return;
      const state = filterByDate(graphData.nodes, graphData.edges, date);
      setFilteredNodes(state.visibleNodes);
      setFilteredEdges(state.visibleEdges);
      setNodeOpacities(state.nodeOpacities);
    },
    [graphData],
  );

  const onTemporalToggle = useCallback(
    (enabled: boolean) => {
      setTemporalEnabled(enabled);
      if (!enabled && graphData) {
        setFilteredNodes(graphData.nodes);
        setFilteredEdges(graphData.edges);
        setNodeOpacities(new Map());
      }
    },
    [graphData],
  );

  const closeDetail = useCallback(() => {
    setSelectedNode(null);
  }, []);

  if (isLoading && !graphData) {
    return <LoadingSpinner label={t('graph.loadingGraph')} className="h-full" />;
  }

  if (!graphData || graphData.nodes.length === 0) {
    return (
      <EmptyState icon="◈" title={t('graph.emptyGraph')} description={t('graph.emptyHint')} className="h-full" />
    );
  }

  return (
    <div className="h-full flex flex-col relative">
      <div className="absolute top-4 left-4 right-4 z-10 flex gap-2">
        <form onSubmit={onSearch} className="flex-1 flex gap-2">
          <SearchInput
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t('graph.searchPlaceholder')}
            aria-label={t('graph.searchPlaceholder')}
            onSubmit={() => setActiveQuery(searchQuery || undefined)}
            className="shadow-md"
          />
          <button
            type="submit"
            className="px-4 py-2 rounded-xl text-sm bg-primary/10 text-primary hover:bg-primary/20 transition shadow-md"
          >
            {t('common.search')}
          </button>
        </form>
      </div>

      <div className="absolute top-16 left-4 z-10 flex items-center gap-2 text-xs text-muted-foreground">
        <span>{t('graph.nodesCount', { nodes: filteredNodes.length })}</span>
        <span>·</span>
        <span>{t('graph.edgesCount', { edges: filteredEdges.length })}</span>
        {temporalEnabled && <Badge variant="warning">{t('graph.temporalActive')}</Badge>}
      </div>

      <div className="flex-1">
        <Graph3D
          nodes={filteredNodes}
          edges={filteredEdges}
          centerId={graphData.center_id}
          events={events}
          nodeOpacities={nodeOpacities}
          onSelect={onNodeSelect}
        />
      </div>

      <TimeSlider nodes={graphData.nodes} onDateChange={onDateChange} onToggle={onTemporalToggle} />

      {selectedMemory && (
        <div className="absolute right-0 top-0 h-full w-96 bg-card/95 backdrop-blur-xl border-l border-border p-4 overflow-y-auto z-20 transition-transform duration-300">
          <div className="flex justify-between items-center mb-3">
            <h3 className="text-sm font-semibold text-foreground">{t('graph.selected')}</h3>
            <Button variant="ghost" size="sm" onClick={closeDetail} aria-label={t('common.close')}>
              ×
            </Button>
          </div>
          <MemoryDetail memory={selectedMemory} onUpdate={closeDetail} onClose={closeDetail} />
        </div>
      )}
    </div>
  );
}
