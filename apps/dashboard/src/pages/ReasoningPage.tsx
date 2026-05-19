import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { ConfidenceRing } from '@/components/ui/confidence-ring';
import { EmptyState } from '@/components/ui/empty-state';
import { InfoTooltip } from '@/components/ui/info-tooltip';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { api } from '@/stores/api';
import { EVENT, track, useTrackPageView } from '@/stores/telemetry';
import type { DeepReferenceResult } from '@/types';

/**
 * Deep reasoning page. Wraps `POST /api/deep_reference` (backed by
 * `tools::cross_reference::execute`) — the reasoning pipeline that
 * surfaces a recommended answer, supporting evidence, detected
 * contradictions, temporal supersession, and dream insights for a
 * natural-language query.
 *
 * Not to be confused with the 8-stage search pipeline visualised on
 * Settings — that one ranks individual results. Reasoning runs on top:
 * it classifies the intent (FactCheck / Timeline / RootCause /
 * Comparison / Synthesis), then weighs candidates with FSRS-6 trust
 * scoring, contradiction detection and temporal supersession.
 *
 * Use this page for questions a single search query can't answer
 * cleanly: "why did we decide X?", "what contradicts Y?",
 * "how has Z evolved?". The page is intentionally manual-fire — no
 * useQuery / staleTime caching — because each run is an expensive
 * cognitive cycle the user should consciously trigger.
 */
export function ReasoningPage() {
  useTrackPageView('reasoning');
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [depth, setDepth] = useState(20);
  const [result, setResult] = useState<DeepReferenceResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const onSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    track(EVENT.reasoning_run, { depth, queryLength: query.trim().length });
    setLoading(true);
    setError(null);
    try {
      const res = await api.deepReference(query, depth);
      setResult(res);
    } catch (e) {
      setError(e instanceof Error ? e : new Error(t('reasoning.errorGeneric')));
      setResult(null);
    } finally {
      setLoading(false);
    }
  };

  // ConfidenceRing handles the colour-tone derivation internally so we don't
  // have to mirror the thresholds here. Keeping the percentage around for the
  // toast/Badge fallback paths only.
  const confidencePct = result ? Math.round(result.confidence * 100) : 0;

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full max-w-4xl mx-auto">
      <header className="space-y-1">
        <h1 className="text-lg font-bold text-foreground">{t('reasoning.title')}</h1>
        <p className="text-xs text-muted-foreground">{t('reasoning.subtitle')}</p>
      </header>

      <Card>
        <form onSubmit={onSubmit} className="space-y-3">
          <div>
            <label htmlFor="reasoning-query" className="text-xs text-muted-foreground block mb-1">
              {t('reasoning.queryLabel')}
            </label>
            <textarea
              id="reasoning-query"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('reasoning.queryPlaceholder')}
              rows={3}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
          <div className="flex items-end gap-3">
            <div className="flex-1">
              <label htmlFor="reasoning-depth" className="text-xs text-muted-foreground block mb-1">
                {t('reasoning.depthLabel', { count: depth })}
              </label>
              <input
                id="reasoning-depth"
                type="range"
                min={5}
                max={50}
                step={5}
                value={depth}
                onChange={(e) => setDepth(Number(e.target.value))}
                className="w-full"
              />
            </div>
            <Button type="submit" variant="dream" disabled={loading || !query.trim()}>
              {loading ? t('common.loading') : t('reasoning.run')}
            </Button>
          </div>
          <p className="text-[11px] text-muted-foreground/80">{t('reasoning.depthHint')}</p>
        </form>
      </Card>

      {error && <QueryErrorPanel error={error} onRetry={() => setError(null)} />}

      {loading && <LoadingSpinner label={t('reasoning.loading')} />}

      {!loading && !result && !error && (
        <EmptyState icon="?" title={t('reasoning.emptyTitle')} description={t('reasoning.emptyHint')} />
      )}

      {result && (
        <div className="space-y-4">
          {/* Top-line answer. The confidence ring sits on the right rail so a
              glance at the colour tells the reader whether to trust the
              recommended answer before they read it. */}
          <Card className="border-primary/30 bg-primary/5">
            <div className="flex items-start justify-between gap-3 mb-2">
              <div className="flex items-center gap-2 flex-wrap min-w-0">
                <Badge variant="secondary">
                  {t(`reasoning.intent.${result.intent}`, { defaultValue: result.intent })}
                </Badge>
                <InfoTooltip
                  content={t(
                    'reasoning.intentTooltip',
                    'Detected intent — fact-check, timeline, root-cause, comparison, or synthesis. Drives which evidence the engine prioritises.',
                  )}
                />
                <span className="text-xs text-muted-foreground">
                  {t('reasoning.analyzed', { count: Number(result.memoriesAnalyzed) })}
                </span>
              </div>
              <ConfidenceRing
                value={result.confidence}
                size={56}
                ariaLabel={t('reasoning.confidence', { pct: confidencePct })}
              />
            </div>
            <p className="text-sm text-foreground whitespace-pre-line leading-relaxed">{result.reasoning}</p>
            {result.recommended && (
              <div className="mt-3 pt-3 border-t border-border/50 flex items-start gap-3">
                <ConfidenceRing
                  value={result.recommended.trust}
                  size={40}
                  ariaLabel={t('reasoning.trustScore', {
                    pct: Math.round(result.recommended.trust * 100),
                  })}
                />
                <div className="min-w-0 flex-1">
                  <p className="text-xs uppercase tracking-wider text-muted-foreground font-medium mb-1">
                    {t('reasoning.topAnswer')}
                  </p>
                  <p className="text-sm text-foreground line-clamp-4">{result.recommended.content}</p>
                  <p className="text-[11px] text-muted-foreground/80 mt-1.5">
                    {t('reasoning.trustScore', {
                      pct: Math.round(result.recommended.trust * 100),
                    })}
                  </p>
                </div>
              </div>
            )}
          </Card>

          {/* Contradictions — only render when present */}
          {result.contradictions.length > 0 && (
            <Card className="border-amber-500/30">
              <h2 className="text-xs uppercase tracking-wider font-semibold text-amber-600 dark:text-amber-400 mb-2">
                {t('reasoning.contradictionsTitle', { count: result.contradictions.length })}
              </h2>
              <ul className="space-y-2">
                {result.contradictions.map((c) => (
                  <li key={`${c.memoryA}-${c.memoryB}`} className="text-xs space-y-1">
                    <div className="flex items-start gap-2">
                      <Badge variant="outline" className="shrink-0 text-[10px]">
                        A
                      </Badge>
                      <span className="text-foreground line-clamp-2">{c.contentA}</span>
                    </div>
                    <div className="flex items-start gap-2">
                      <Badge variant="outline" className="shrink-0 text-[10px]">
                        B
                      </Badge>
                      <span className="text-foreground line-clamp-2">{c.contentB}</span>
                    </div>
                    <div className="flex gap-3 text-[10px] text-muted-foreground pl-6">
                      <span>trust A: {(c.trustA * 100).toFixed(0)}%</span>
                      <span>trust B: {(c.trustB * 100).toFixed(0)}%</span>
                    </div>
                  </li>
                ))}
              </ul>
            </Card>
          )}

          {/* Superseded memories */}
          {result.superseded.length > 0 && (
            <Card>
              <h2 className="text-xs uppercase tracking-wider font-semibold text-muted-foreground mb-2">
                {t('reasoning.supersededTitle', { count: result.superseded.length })}
              </h2>
              <ul className="space-y-1.5">
                {result.superseded.map((s) => (
                  <li key={`${s.supersededId}-${s.supersededBy}`} className="text-xs text-muted-foreground">
                    <span className="line-through">{s.supersededId.slice(0, 8)}</span>
                    {' → '}
                    <span className="text-foreground">{s.supersededBy.slice(0, 8)}</span>
                    <span className="ml-2 text-muted-foreground/70">({s.reason})</span>
                  </li>
                ))}
              </ul>
            </Card>
          )}

          {/* Dream insights — surface "the system noticed this earlier" */}
          {result.dreamInsights.length > 0 && (
            <Card className="border-violet-500/20">
              <h2 className="text-xs uppercase tracking-wider font-semibold text-violet-500 mb-2">
                {t('reasoning.dreamInsightsTitle', { count: result.dreamInsights.length })}
              </h2>
              <ul className="space-y-2">
                {result.dreamInsights.map((insight) => (
                  <li key={insight.insight} className="text-xs flex items-start gap-2">
                    <ConfidenceRing
                      value={insight.confidence}
                      size={32}
                      ariaLabel={t('reasoning.insightConfidence', {
                        pct: Math.round(insight.confidence * 100),
                      })}
                    />
                    <p className="text-foreground flex-1 min-w-0">{insight.insight}</p>
                  </li>
                ))}
              </ul>
            </Card>
          )}

          {/* Evidence list */}
          {result.evidence.length > 0 && (
            <Card>
              <h2 className="text-xs uppercase tracking-wider font-semibold text-muted-foreground mb-2">
                {t('reasoning.evidenceTitle', { count: result.evidence.length })}
              </h2>
              <ol className="space-y-2">
                {result.evidence.slice(0, 10).map((ev, i) => (
                  <li key={ev.id} className="flex items-start gap-3 text-xs">
                    <span className="w-5 h-5 rounded-full bg-primary/15 text-primary text-[10px] flex items-center justify-center shrink-0 tabular-nums">
                      {i + 1}
                    </span>
                    <div className="flex-1 min-w-0">
                      <p className="text-foreground line-clamp-2">{ev.content}</p>
                      <div className="flex flex-wrap gap-2 mt-1 text-[10px] text-muted-foreground">
                        <Badge variant="outline">{ev.nodeType}</Badge>
                        <span>trust {(ev.trust * 100).toFixed(0)}%</span>
                        <span>retention {(ev.retention * 100).toFixed(0)}%</span>
                        <span>reps {ev.reps}</span>
                        {ev.lapses > 0 && <span className="text-amber-500">lapses {ev.lapses}</span>}
                      </div>
                    </div>
                  </li>
                ))}
              </ol>
            </Card>
          )}

          {/* Stages that ran / didn't */}
          <p className="text-[11px] text-muted-foreground/70 text-center">
            {t('reasoning.stagesFooter', {
              activation: result.stagesCompleted.spreadingActivation ? t('common.yes') : t('common.no'),
              dream: result.stagesCompleted.dreamInsights ? t('common.yes') : t('common.no'),
            })}
          </p>
        </div>
      )}
    </div>
  );
}
