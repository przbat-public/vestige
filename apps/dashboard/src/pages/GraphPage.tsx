import { useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Graph3D } from '@/components/Graph3D';
import { GraphHelpOverlay } from '@/components/GraphHelpOverlay';
import { MemoryDetail } from '@/components/memories/MemoryDetail';
import { SectionErrorBoundary } from '@/components/SectionErrorBoundary';
import { TagLegend } from '@/components/TagLegend';
import { TimeSlider } from '@/components/TimeSlider';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { NativeSelect } from '@/components/ui/native-select';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SearchInput } from '@/components/ui/search-input';
import { Sheet } from '@/components/ui/sheet';
import { filterByDate } from '@/graph/temporal';
import { useDashboardLimits } from '@/hooks/use-dashboard-limits';
import { useReducedMotion } from '@/hooks/use-reduced-motion';
import { useSelectionHistory } from '@/hooks/use-selection-history';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useTrackPageView } from '@/stores/telemetry';
import { useWebSocket } from '@/stores/websocket';
import type { GraphEdge, GraphNode } from '@/types';

// Module-level frozen singleton — passing a fresh `new Map()` from inside
// the component would change identity every render and bust Graph3D's
// `useEffect` deps (the renderer keys off `nodeOpacities`). The frozen
// empty Map is the same object forever.
const EMPTY_OPACITIES: ReadonlyMap<string, number> = Object.freeze(new Map<string, number>());

export function GraphPage() {
  useTrackPageView('graph');
  const { t } = useTranslation();
  const events = useWebSocket((s) => s.events);
  const isDreaming = useWebSocket((s) => s.isDreaming);
  const reducedMotion = useReducedMotion();

  const limits = useDashboardLimits();
  const [searchQuery, setSearchQuery] = useState('');
  const [activeQuery, setActiveQuery] = useState<string | undefined>();
  // Seed from server-driven defaults; the user can still expand up to
  // `graphMaxNodesMax` via the slider. Keeping the initial value at 150
  // (above the canonical default of 50) preserves the previous UX —
  // the graph page benefits from a denser snapshot than other pages.
  const [maxNodes, setMaxNodes] = useState(150);
  const [depth, setDepth] = useState<1 | 2 | 3>(2);
  const [tagFilter, setTagFilter] = useState('');
  const [colorMode, setColorMode] = useState<'type' | 'tag'>('type');
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [keyboardFocusId, setKeyboardFocusId] = useState<string | null>(null);
  const [temporalEnabled, setTemporalEnabled] = useState(false);
  const [temporalDate, setTemporalDate] = useState<Date | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);

  // Selection history — back/forward through the user's exploration path.
  // Distinct from the browser history (which we never touch). Capped at
  // 50 because we're just an in-memory stack, not a paged audit log.
  const history = useSelectionHistory({ maxEntries: 50 });

  const graphParams = { max_nodes: maxNodes, depth, query: activeQuery };
  const {
    data: graphData,
    isLoading,
    isError,
    error: graphError,
    refetch: refetchGraph,
  } = useQuery({
    queryKey: queryKeys.graph(graphParams),
    queryFn: () => api.graph(graphParams),
  });

  const { data: selectedMemory } = useQuery({
    queryKey: queryKeys.memory(selectedNode ?? ''),
    // `selectedNode` is narrowed by `enabled` below, but TS can't see that
    // through the queryFn boundary — use the non-null assertion inline so
    // there's no `as string` cast hiding stale-null bugs.
    queryFn: () => {
      if (!selectedNode) throw new Error('queryFn called without selectedNode');
      return api.memories.get(selectedNode);
    },
    enabled: !!selectedNode,
  });

  const nodeOpacities = useMemo(() => {
    if (!graphData || !temporalEnabled || !temporalDate) return EMPTY_OPACITIES;
    return filterByDate(graphData.nodes, graphData.edges, temporalDate).nodeOpacities;
  }, [graphData, temporalEnabled, temporalDate]);

  // Tag filter is applied client-side over the already-fetched subgraph
  // (the backend search is full-text, so we'd lose the spatial layout if we
  // re-queried). Memoized so we only recompute when inputs change.
  const filteredGraph = useMemo(() => {
    const baseNodes: GraphNode[] = graphData?.nodes ?? [];
    const baseEdges: GraphEdge[] = graphData?.edges ?? [];
    const tag = tagFilter.trim().toLowerCase();
    if (!tag) return { nodes: baseNodes, edges: baseEdges };
    const allowed = new Set(
      baseNodes.filter((n) => n.tags?.some((t) => t.toLowerCase().includes(tag))).map((n) => n.id),
    );
    const nodes = baseNodes.filter((n) => allowed.has(n.id));
    const edges = baseEdges.filter((e) => allowed.has(e.source) && allowed.has(e.target));
    return { nodes, edges };
  }, [graphData, tagFilter]);

  const displayNodes: GraphNode[] = filteredGraph.nodes;
  const displayEdges: GraphEdge[] = filteredGraph.edges;

  const onSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setActiveQuery(searchQuery || undefined);
  };

  const onNodeSelect = useCallback(
    (nodeId: string) => {
      setSelectedNode(nodeId);
      history.push(nodeId);
    },
    [history],
  );

  const onKeyboardFocus = useCallback((nodeId: string | null) => {
    setKeyboardFocusId(nodeId);
  }, []);

  const onDeselect = useCallback(() => {
    // Clicking the void only clears the visible selection. We intentionally
    // keep the history stack — the user can still Alt+Left to return to
    // whatever they were inspecting. Matches browser semantics where
    // closing a tab doesn't wipe its session history.
    setSelectedNode(null);
  }, []);

  const onShowHelp = useCallback(() => setHelpOpen(true), []);
  const onCloseHelp = useCallback(() => setHelpOpen(false), []);

  // Alt+Arrow at the page level: graph-scoped back/forward. We intercept
  // before the browser does because the canvas always has focus while
  // the user is exploring. Bubbled from Graph3D's onKeyDown —
  // useGraphKeyboard intentionally lets modifier+arrow events through.
  // We only consume the event when the history can actually move; that
  // way ⌥← in a search input (where canGoBack is false) still does the
  // browser-default "back one word" the user expects.
  const onPageKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (!event.altKey) return;
      if (event.metaKey || event.ctrlKey || event.shiftKey) return;
      if (event.key === 'ArrowLeft' && history.canGoBack) {
        event.preventDefault();
        history.back();
      } else if (event.key === 'ArrowRight' && history.canGoForward) {
        event.preventDefault();
        history.forward();
      }
    },
    [history],
  );

  // Sync history cursor → selection. This is the single source of truth
  // for "what the user just navigated to". push() (called from
  // onNodeSelect) makes the new id current; back()/forward() shift the
  // cursor; in either case the selection follows.
  useEffect(() => {
    if (history.currentId !== null) setSelectedNode(history.currentId);
  }, [history.currentId]);

  const focusedNodeLabel = useMemo(() => {
    if (!keyboardFocusId) return null;
    return displayNodes.find((n) => n.id === keyboardFocusId)?.label ?? keyboardFocusId;
  }, [keyboardFocusId, displayNodes]);

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
    return <QueryErrorPanel className="m-4" error={graphError} onRetry={refetchGraph} />;
  }

  if (!graphData || graphData.nodes.length === 0) {
    return <EmptyState icon="◈" title={t('graph.emptyGraph')} description={t('graph.emptyHint')} className="h-full" />;
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: keyDown on a layout div is intentional — we capture page-level shortcuts (Alt+Arrow) that bubble from the focused canvas. Adding role would mis-describe semantics.
    <div className="h-full flex flex-col relative" onKeyDown={onPageKeyDown}>
      {/* Screen-reader page title — the visible toolbar acts as the visual
          heading, but assistive tech still needs an h1 to anchor the
          document outline. */}
      <h1 className="sr-only">{t('graph.title')}</h1>
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
      </div>

      <div className="absolute top-16 left-4 right-4 z-10 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <span>{t('graph.nodesCount', { nodes: displayNodes.length })}</span>
        <span>·</span>
        <span>{t('graph.edgesCount', { edges: displayEdges.length })}</span>
        {temporalEnabled && <Badge variant="warning">{t('graph.temporalActive')}</Badge>}
        <div className="ml-auto flex flex-wrap items-center gap-1.5">
          <input
            type="text"
            value={tagFilter}
            onChange={(e) => setTagFilter(e.target.value)}
            placeholder={t('graph.tagFilterPlaceholder')}
            aria-label={t('graph.tagFilter')}
            className="bg-card/90 backdrop-blur-sm rounded-lg px-2 py-1 border border-border/50 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring max-w-[160px]"
          />
          <div className="flex items-center gap-1.5 bg-card/90 backdrop-blur-sm rounded-lg px-2 py-1 border border-border/50">
            <span className="text-muted-foreground whitespace-nowrap">{t('graph.depth')}:</span>
            <NativeSelect
              value={depth}
              onChange={(e) => setDepth(Number(e.target.value) as 1 | 2 | 3)}
              aria-label={t('graph.depth')}
              className="text-xs font-medium"
            >
              {[1, 2, 3]
                .filter((d) => d <= limits.graphDepthMax)
                .map((d) => (
                  <option key={d} value={d}>
                    {d}
                  </option>
                ))}
            </NativeSelect>
          </div>
          <div className="flex items-center gap-1.5 bg-card/90 backdrop-blur-sm rounded-lg px-2 py-1 border border-border/50">
            <span className="text-muted-foreground whitespace-nowrap">{t('graph.colorBy')}:</span>
            <NativeSelect
              value={colorMode}
              onChange={(e) => setColorMode(e.target.value as 'type' | 'tag')}
              aria-label={t('graph.colorBy')}
              className="text-xs font-medium"
            >
              <option value="type">{t('graph.colorByType')}</option>
              <option value="tag">{t('graph.colorByTag')}</option>
            </NativeSelect>
          </div>
          <div className="flex items-center gap-1.5 bg-card/90 backdrop-blur-sm rounded-lg px-2 py-1 border border-border/50">
            <span className="text-muted-foreground whitespace-nowrap">{t('graph.maxNodes')}:</span>
            <NativeSelect
              value={maxNodes}
              onChange={(e) => setMaxNodes(Number(e.target.value))}
              aria-label={t('graph.maxNodes')}
              className="text-xs font-medium"
            >
              {[50, 100, 150, 200, 300, 500, 1000]
                .filter((n) => n <= limits.graphMaxNodesMax)
                .map((n) => (
                  <option key={n} value={n}>
                    {t('graph.nodeOption', { count: n })}
                  </option>
                ))}
            </NativeSelect>
          </div>
        </div>
      </div>

      <SectionErrorBoundary fallbackTitle={t('graph.title')}>
        <div className="flex-1 relative">
          <Graph3D
            nodes={displayNodes}
            edges={displayEdges}
            centerId={graphData.centerId}
            events={events}
            nodeOpacities={nodeOpacities}
            isDreaming={isDreaming}
            reducedMotion={reducedMotion}
            colorMode={colorMode}
            onSelect={onNodeSelect}
            onKeyboardFocus={onKeyboardFocus}
            onDeselect={onDeselect}
            onShowHelp={onShowHelp}
            centerOnId={selectedNode}
          />
          {/* Single aria-live region for the page — the canvas itself can't
              host announcements because its DOM is just a Three.js host
              <div>. We use polite so SR users aren't interrupted by every
              arrow-key tick. */}
          <div className="sr-only" role="status" aria-live="polite">
            {focusedNodeLabel ? t('graph.focusAnnouncement', { label: focusedNodeLabel }) : t('graph.focusCleared')}
          </div>
          {colorMode === 'tag' && <TagLegend nodes={displayNodes} />}
          {tagFilter.trim().length > 0 && displayNodes.length === 0 && (
            // Filter wiped every node. We keep the underlying graphData
            // intact (so clearing the filter is instant), but tell the
            // user explicitly what happened — a silent empty canvas is
            // indistinguishable from a backend bug.
            <div
              className="absolute inset-0 flex items-center justify-center pointer-events-none z-10"
              role="status"
              aria-live="polite"
            >
              <div className="pointer-events-auto bg-card/95 backdrop-blur-md border border-border rounded-2xl px-6 py-5 shadow-lg max-w-md text-center space-y-2">
                <h2 className="text-sm font-semibold text-foreground">{t('graph.noMatch')}</h2>
                <p className="text-xs text-muted-foreground">{t('graph.noMatchHint')}</p>
                <Button variant="secondary" size="sm" onClick={() => setTagFilter('')}>
                  {t('graph.clearFilter')}
                </Button>
              </div>
            </div>
          )}
        </div>
      </SectionErrorBoundary>
      {isDreaming && (
        <div
          className="absolute top-16 left-1/2 -translate-x-1/2 z-10 flex items-center gap-2 bg-purple-500/15 border border-purple-400/30 backdrop-blur-sm rounded-full px-3 py-1 text-xs text-purple-200 shadow-md"
          role="status"
          aria-live="polite"
        >
          <span className="inline-block w-2 h-2 rounded-full bg-purple-400 animate-pulse" aria-hidden="true" />
          <span>{t('graph.dreamingBadge')}</span>
        </div>
      )}

      <TimeSlider nodes={graphData.nodes} onDateChange={onDateChange} onToggle={onTemporalToggle} />

      <GraphHelpOverlay open={helpOpen} onClose={onCloseHelp} />

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
