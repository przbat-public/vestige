import type { GraphEdge, GraphNode } from '@/types';

export interface TemporalState {
  visibleNodes: GraphNode[];
  visibleEdges: GraphEdge[];
  nodeOpacities: Map<string, number>;
}

/**
 * Compute per-node opacities based on a temporal cutoff date.
 * All nodes/edges are kept — only opacities change. This prevents
 * force-simulation rebuilds that cause graph jitter during playback.
 *
 * Nodes created after cutoffDate get opacity 0 (invisible).
 * Nodes near the cutoff get a 1-day fade-in ramp.
 * Edges where either endpoint is invisible also get opacity 0.
 */
export function filterByDate(nodes: GraphNode[], edges: GraphEdge[], cutoffDate: Date): TemporalState {
  const cutoff = cutoffDate.getTime();
  const nodeOpacities = new Map<string, number>();

  for (const node of nodes) {
    const created = new Date(node.createdAt).getTime();
    if (created > cutoff) {
      nodeOpacities.set(node.id, 0);
    } else {
      const age = cutoff - created;
      const fadeWindow = 24 * 60 * 60 * 1000;
      nodeOpacities.set(node.id, age < fadeWindow ? 0.3 + 0.7 * (age / fadeWindow) : 1.0);
    }
  }

  return { visibleNodes: nodes, visibleEdges: edges, nodeOpacities };
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
