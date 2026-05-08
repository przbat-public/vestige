import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { getNodeColor } from '@/graph/nodes';
import { isDarkMode } from '@/graph/theme';
import type { GraphNode } from '@/types';

interface TagLegendProps {
  nodes: GraphNode[];
  /** How many tags to show. Defaults to 10 — fits the corner without scrolling. */
  limit?: number;
}

/**
 * Legend listing the top primary-tag colours currently visible on the graph.
 *
 * Shown only when `colorMode === 'tag'` is active. Counts how many nodes carry
 * each first-tag (the same key the dream/consolidation engine uses for
 * cross-domain detection) and renders a swatch with the matching HSL colour.
 *
 * Untagged nodes are not listed — they all share the neutral grey, which is
 * documented inline as "(no tag)".
 *
 * Renders nothing when there are no tagged nodes; never blocks rendering of
 * the underlying graph.
 */
export function TagLegend({ nodes, limit = 10 }: TagLegendProps) {
  const { t } = useTranslation();
  const dark = isDarkMode();

  const entries = useMemo(() => {
    const counts = new Map<string, number>();
    let untagged = 0;
    for (const node of nodes) {
      const primary = node.tags?.[0];
      if (!primary) {
        untagged += 1;
        continue;
      }
      counts.set(primary, (counts.get(primary) ?? 0) + 1);
    }
    const sorted = Array.from(counts.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, limit);
    return { sorted, untagged };
  }, [nodes, limit]);

  if (entries.sorted.length === 0) return null;

  return (
    <details
      className="absolute bottom-4 right-4 z-10 max-w-[14rem] bg-card/90 backdrop-blur-sm rounded-lg border border-border/50 shadow-md text-xs"
      // Default open so users see the legend when they switch into tag mode;
      // they can collapse it manually if it's in the way.
      open
    >
      <summary className="cursor-pointer select-none px-3 py-2 font-medium text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg">
        {t('graph.tagLegendTitle')}
        <span className="ml-1.5 text-[10px] tabular-nums opacity-70">
          ({entries.sorted.length})
        </span>
      </summary>
      <ul className="px-3 pb-3 pt-1 space-y-1 max-h-60 overflow-y-auto">
        {entries.sorted.map(([tag, count]) => {
          // We rebuild the colour through the same helper the renderer uses,
          // so legend ↔ scene stay in sync if the helper ever changes.
          const sample: GraphNode = {
            id: `legend-${tag}`,
            label: tag,
            type: 'fact',
            retention: 1,
            tags: [tag],
            createdAt: '',
            updatedAt: '',
            isCenter: false,
          };
          const color = getNodeColor(sample, 'tag', dark);
          return (
            <li key={tag} className="flex items-center gap-2">
              <span
                className="inline-block w-3 h-3 rounded-full flex-shrink-0"
                style={{ backgroundColor: color }}
                aria-hidden="true"
              />
              <span className="text-muted-foreground truncate flex-1" title={tag}>
                {tag}
              </span>
              <span className="text-foreground tabular-nums text-[10px] opacity-70">
                {count}
              </span>
            </li>
          );
        })}
        {entries.untagged > 0 && (
          <li className="flex items-center gap-2 border-t border-border pt-1.5 mt-1.5">
            <span
              className="inline-block w-3 h-3 rounded-full flex-shrink-0 bg-[#8B95A5]"
              aria-hidden="true"
            />
            <span className="text-muted-foreground italic flex-1">
              {t('graph.tagLegendUntagged')}
            </span>
            <span className="text-foreground tabular-nums text-[10px] opacity-70">
              {entries.untagged}
            </span>
          </li>
        )}
      </ul>
    </details>
  );
}
