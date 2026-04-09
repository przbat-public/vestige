import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router';
import { ImportanceScorer } from '@/components/ImportanceScorer';
import { Alert } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { SearchInput } from '@/components/ui/search-input';
import { api } from '@/stores/api';
import type { ExploreResult, Memory } from '@/types';

type ExploreMode = 'associations' | 'chains' | 'bridges';

const MODE_KEYS: Record<ExploreMode, { icon: string; labelKey: string }> = {
  associations: { icon: '◎', labelKey: 'explore.associations' },
  chains: { icon: '⟿', labelKey: 'explore.chain' },
  bridges: { icon: '⬡', labelKey: 'explore.bridges' },
};

export function ExplorePage() {
  const { t } = useTranslation();
  const [searchParams] = useSearchParams();
  const [mode, setMode] = useState<ExploreMode>('associations');

  const [searchQuery, setSearchQuery] = useState('');
  const [targetQuery, setTargetQuery] = useState('');
  const [sourceMemory, setSourceMemory] = useState<Memory | null>(null);
  const [targetMemory, setTargetMemory] = useState<Memory | null>(null);
  const [results, setResults] = useState<ExploreResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const explore = useCallback(
    async (source: Memory, target?: Memory, m?: ExploreMode) => {
      setLoading(true);
      setError('');
      try {
        const activeMode = m ?? mode;
        const toId = (activeMode === 'chains' || activeMode === 'bridges') && target ? target.id : undefined;
        const res = await api.explore(source.id, activeMode, toId);
        setResults(res.results);
      } catch (e) {
        setResults([]);
        setError(e instanceof Error ? e.message : t('explore.errorExploration'));
      } finally {
        setLoading(false);
      }
    },
    [mode, t],
  );

  const findSource = async () => {
    if (!searchQuery.trim()) return;
    setLoading(true);
    setError('');
    try {
      const res = await api.search(searchQuery, 1);
      if (res.results.length > 0) {
        setSourceMemory(res.results[0]);
        await explore(res.results[0]);
      } else {
        setError(t('common.noResults'));
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : t('explore.errorSearch'));
    } finally {
      setLoading(false);
    }
  };

  const findTarget = async () => {
    if (!targetQuery.trim()) return;
    setLoading(true);
    setError('');
    try {
      const res = await api.search(targetQuery, 1);
      if (res.results.length > 0) {
        setTargetMemory(res.results[0]);
        if (sourceMemory) await explore(sourceMemory, res.results[0]);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : t('explore.errorSearch'));
    } finally {
      setLoading(false);
    }
  };

  const switchMode = (m: ExploreMode) => {
    setMode(m);
    if (sourceMemory) void explore(sourceMemory, targetMemory ?? undefined, m);
  };

  useEffect(() => {
    const fromId = searchParams.get('from');
    if (!fromId) return;
    let cancelled = false;
    (async () => {
      try {
        const mem = await api.memories.get(fromId);
        if (cancelled) return;
        setSourceMemory(mem);
        setSearchQuery(mem.content.slice(0, 60));
        await explore(mem);
      } catch {
        if (!cancelled) setError(t('explore.errorLoadMemory', { id: fromId.slice(0, 8) }));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [searchParams, explore, t]);

  return (
    <div className="p-4 space-y-6 overflow-y-auto h-full w-full">
      <h1 className="text-xl text-foreground font-semibold">{t('explore.title')}</h1>

      {error && <Alert variant="destructive">{error}</Alert>}

      <div className="grid grid-cols-3 gap-2">
        {(Object.keys(MODE_KEYS) as ExploreMode[]).map((m) => (
          <button
            type="button"
            key={m}
            onClick={() => switchMode(m)}
            className={`flex flex-col items-center gap-1 p-3 rounded-xl text-sm transition border ${
              mode === m
                ? 'border-primary bg-primary/10 text-primary'
                : 'border-border bg-card text-muted-foreground hover:bg-accent'
            }`}
            aria-pressed={mode === m}
          >
            <span className="text-xl">{MODE_KEYS[m].icon}</span>
            <span className="font-medium">{t(MODE_KEYS[m].labelKey)}</span>
          </button>
        ))}
      </div>

      <div className="space-y-3">
        <span className="text-xs text-muted-foreground font-medium">{t('explore.fromLabel')}</span>
        <div className="flex gap-2">
          <SearchInput
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onSubmit={findSource}
            placeholder={t('explore.searchPlaceholder')}
            aria-label={t('explore.searchPlaceholder')}
          />
          <Button onClick={findSource}>{t('common.search')}</Button>
        </div>
      </div>

      {sourceMemory && (
        <Card className="border-primary/20">
          <div className="text-xs text-primary mb-1 uppercase tracking-wider font-medium">{t('explore.fromLabel')}</div>
          <p className="text-sm text-foreground">{sourceMemory.content.slice(0, 200)}</p>
          <div className="flex gap-2 mt-1.5">
            <Badge variant="secondary">
              {t(`nodeTypes.${sourceMemory.nodeType}`, { defaultValue: sourceMemory.nodeType })}
            </Badge>
            <span className="text-xs text-muted-foreground">
              {t('explore.retention', { value: (sourceMemory.retentionStrength * 100).toFixed(0) })}
            </span>
          </div>
        </Card>
      )}

      {(mode === 'chains' || mode === 'bridges') && (
        <>
          <div className="space-y-3">
            <span className="text-xs text-muted-foreground font-medium">{t('explore.toLabel')}</span>
            <div className="flex gap-2">
              <SearchInput
                value={targetQuery}
                onChange={(e) => setTargetQuery(e.target.value)}
                onSubmit={findTarget}
                placeholder={t('explore.searchPlaceholder')}
                aria-label={t('explore.toLabel')}
              />
              <Button variant="dream" onClick={findTarget}>
                {t('common.search')}
              </Button>
            </div>
          </div>
          {targetMemory && (
            <Card className="border-violet-500/20">
              <div className="text-xs text-violet-500 mb-1 uppercase tracking-wider font-medium">
                {t('explore.toLabel')}
              </div>
              <p className="text-sm text-foreground">{targetMemory.content.slice(0, 200)}</p>
            </Card>
          )}
        </>
      )}

      {sourceMemory &&
        (loading ? (
          <LoadingSpinner label={t('common.loading')} />
        ) : results.length > 0 ? (
          <div className="space-y-4">
            <h2 className="text-sm text-foreground font-semibold">
              {results.length} {t('explore.associations')}
            </h2>
            <div className="space-y-2">
              {results.map((assoc, i) => (
                <Card key={`${String(assoc.content ?? '')}|${i}`} className="flex items-start gap-3">
                  <div className="w-6 h-6 rounded-full bg-primary/15 text-primary text-xs flex items-center justify-center flex-shrink-0 mt-0.5 tabular-nums">
                    {i + 1}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-sm text-foreground line-clamp-2">{String(assoc.content ?? '')}</p>
                    <div className="flex flex-wrap gap-2 mt-1.5 text-xs">
                      {assoc.nodeType && <Badge variant="secondary">{assoc.nodeType}</Badge>}
                      {assoc.score != null && (
                        <span className="text-muted-foreground">
                          {t('explore.scoreLabel', { value: Number(assoc.score).toFixed(3) })}
                        </span>
                      )}
                      {assoc.similarity != null && (
                        <span className="text-muted-foreground">
                          {t('explore.matchLabel', { value: (Number(assoc.similarity) * 100).toFixed(0) })}
                        </span>
                      )}
                      {assoc.connectionType && <Badge>{assoc.connectionType}</Badge>}
                    </div>
                  </div>
                </Card>
              ))}
            </div>
          </div>
        ) : (
          <EmptyState icon="◬" title={t('explore.noResults')} description={t('explore.noResultsHint')} />
        ))}

      <ImportanceScorer />
    </div>
  );
}
