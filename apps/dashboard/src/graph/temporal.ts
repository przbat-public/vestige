import type { GraphEdge, GraphNode } from '@/types';

export interface TemporalState {
  visibleNodes: GraphNode[];
  visibleEdges: GraphEdge[];
  nodeOpacities: Map<string, number>;
}

/**
 * Filter nodes and edges by a temporal cutoff date.
 * Nodes are visible if createdAt <= cutoffDate.
 * Edges are visible if both endpoints are visible.
 */
export function filterByDate(nodes: GraphNode[], edges: GraphEdge[], cutoffDate: Date): TemporalState {
  const cutoff = cutoffDate.getTime();
  const visibleNodeIds = new Set<string>();
  const nodeOpacities = new Map<string, number>();

  const visibleNodes = nodes.filter((node) => {
    const created = new Date(node.createdAt).getTime();
    if (created <= cutoff) {
      visibleNodeIds.add(node.id);

      // Nodes created near the cutoff date get a fade-in opacity
      const age = cutoff - created;
      const fadeWindow = 24 * 60 * 60 * 1000; // 1 day fade window
      const opacity = age < fadeWindow ? 0.3 + 0.7 * (age / fadeWindow) : 1.0;
      nodeOpacities.set(node.id, opacity);

      return true;
    }
    return false;
  });

  const visibleEdges = edges.filter((edge) => visibleNodeIds.has(edge.source) && visibleNodeIds.has(edge.target));

  return { visibleNodes, visibleEdges, nodeOpacities };
}

/**
 * Get the date range from a set of nodes (oldest to newest).
 */
export function getDateRange(nodes: GraphNode[]): { oldest: Date; newest: Date } {
  if (nodes.length === 0) {
    const now = new Date();
    return { oldest: now, newest: now };
  }

  let oldest = Infinity;
  let newest = -Infinity;

  for (const node of nodes) {
    const ts = new Date(node.createdAt).getTime();
    if (ts < oldest) oldest = ts;
    if (ts > newest) newest = ts;
  }

  return {
    oldest: new Date(oldest),
    newest: new Date(newest),
  };
}
