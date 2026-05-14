import { useQueries } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { NODE_TYPE_COLORS, retentionColor } from '@/types';

interface InsightSourcesPanelProps {
  /** Memory ids the parent insight points at. May be empty. */
  memoryIds: string[];
  /**
   * Optional cap on how many sources we render expanded. Useful for
   * insights with long source lists where the user mostly wants a quick
   * preview.
   */
  initialLimit?: number;
}

/**
 * Renders the "memory sources" provenance for a reflect insight —
 * RAG-style transparency. Given a list of memory ids, it lazy-fetches
 * each one (cached individually under `queryKeys.memory(id)` so other
 * pages reuse the result) and shows a content preview, retention badge,
 * and node-type chip.
 *
 * Explicit collapse/expand button keeps the briefing skimmable when an
 * insight has 20+ contradictory pairs — show the first 5, click to see
 * the rest.
 *
 * Why useQueries instead of one combined endpoint: the dashboard already
 * has per-memory caching, and reusing it means an insight pointing at a
 * memory the user just opened in the Memory drawer renders instantly.
 * The trade-off is N parallel requests; in practice insights point at
 * fewer than 10 memories so this is fine.
 */
export function InsightSourcesPanel({ memoryIds, initialLimit = 5 }: InsightSourcesPanelProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  if (memoryIds.length === 0) {
    return <p className="text-[10px] text-muted-foreground italic">{t('insightSources.noSources')}</p>;
  }

  const visibleIds = expanded ? memoryIds : memoryIds.slice(0, initialLimit);
  const remaining = memoryIds.length - visibleIds.length;

  return (
    <SourcesList ids={visibleIds} hiddenCount={remaining} expanded={expanded} onToggle={() => setExpanded((p) => !p)} />
  );
}

interface SourcesListProps {
  ids: string[];
  hiddenCount: number;
  expanded: boolean;
  onToggle: () => void;
}

function SourcesList({ ids, hiddenCount, expanded, onToggle }: SourcesListProps) {
  const { t } = useTranslation();

  // useQueries is the right primitive here: it caches each id under the
  // canonical `memory(id)` key, so opening the same memory in
  // MemoriesPage afterwards is free.
  const queries = useQueries({
    queries: ids.map((id) => ({
      queryKey: queryKeys.memory(id),
      queryFn: () => api.memories.get(id),
      // Generous staleTime — the briefing isn't a real-time view of
      // these memories, just a "what triggered this insight" reference.
      staleTime: 5 * 60_000,
    })),
  });

  const loading = queries.some((q) => q.isLoading);
  const errored = queries.filter((q) => q.isError);

  return (
    <div className="space-y-1.5">
      {loading && <LoadingSpinner label={t('insightSources.loading')} className="py-2" />}
      {errored.length > 0 && (
        <p className="text-[10px] text-amber-600 dark:text-amber-400">
          {t('insightSources.partialFailure', { count: errored.length })}
        </p>
      )}
      {queries.map((q, i) => {
        const id = ids[i];
        // Loading or errored: render a stub row so the user sees that
        // there *was* a source, even if we couldn't fetch it. This is
        // important for trust — silent omission would look like the
        // insight had fewer sources than it claims.
        if (!q.data) {
          if (q.isLoading) return null;
          return (
            <div key={id} className="text-[10px] text-muted-foreground/70 italic px-2 py-1">
              {t('insightSources.failedRow', { id: id.slice(0, 8) })}
            </div>
          );
        }
        const m = q.data;
        return (
          <div
            key={id}
            className="flex items-start gap-2 px-2 py-1.5 rounded-lg bg-muted/30 border border-border/50 text-xs"
          >
            <span
              aria-hidden="true"
              className="w-1.5 h-1.5 rounded-full mt-1.5 shrink-0"
              style={{ backgroundColor: NODE_TYPE_COLORS[m.nodeType] || '#8B95A5' }}
            />
            <div className="flex-1 min-w-0 space-y-1">
              <p className="text-foreground leading-snug break-words">{m.content.slice(0, 200)}</p>
              <div className="flex items-center gap-2 flex-wrap text-[10px] text-muted-foreground">
                <Badge variant="outline" className="text-[9px] capitalize">
                  {m.nodeType}
                </Badge>
                <span className="tabular-nums" style={{ color: retentionColor(m.retentionStrength) }}>
                  r{m.retentionStrength.toFixed(2)}
                </span>
                <span className="font-mono">{id.slice(0, 8)}</span>
              </div>
            </div>
          </div>
        );
      })}
      {(hiddenCount > 0 || expanded) && (
        <button type="button" onClick={onToggle} className="text-[10px] text-primary hover:underline px-2">
          {expanded ? t('insightSources.collapse') : t('insightSources.expand', { count: hiddenCount })}
        </button>
      )}
    </div>
  );
}
