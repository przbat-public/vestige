import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DreamResultPanel } from '@/components/DreamResultPanel';
import { GovernancePanel } from '@/components/GovernancePanel';
import { MaintenancePanel } from '@/components/MaintenancePanel';
import { PipelineVisualizer } from '@/components/PipelineVisualizer';
import { SectionErrorBoundary } from '@/components/SectionErrorBoundary';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { useMemoryMutations } from '@/hooks/useMemoryMutations';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useTrackPageView } from '@/stores/telemetry';
import { toast } from '@/stores/toast';
import { useWebSocket } from '@/stores/websocket';
import type { ConfidenceResult, ConsolidationResult, DreamResult, ReflectResult } from '@/types';

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: settings page composes seven independent maintenance actions whose results live in local state; splitting them would force a context just to share toasts
export function SettingsPage() {
  useTrackPageView('settings');
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { connected, memoryCount, avgRetention } = useWebSocket();
  const { data: stats } = useQuery({ queryKey: queryKeys.stats, queryFn: api.stats });
  const { data: health } = useQuery({ queryKey: queryKeys.health, queryFn: api.health });
  const { data: distribution } = useQuery({
    queryKey: queryKeys.retentionDistribution,
    queryFn: api.retentionDistribution,
  });
  const [dreamResult, setDreamResult] = useState<DreamResult | null>(null);
  const [consolResult, setConsolResult] = useState<ConsolidationResult | null>(null);
  const [reflectResult, setReflectResult] = useState<ReflectResult | null>(null);
  const [confidenceResult, setConfidenceResult] = useState<ConfidenceResult | null>(null);

  // Mutations for Doubt Mode actions on the confidence-audit list.
  // Locally hide rows after action so the UI feels responsive.
  const [hiddenIds, setHiddenIds] = useState<Set<string>>(new Set());
  const { promote: doubtPromote, demote: doubtDemote } = useMemoryMutations({
    onPromote: () => qc.invalidateQueries({ queryKey: queryKeys.stats }),
    onDemote: () => qc.invalidateQueries({ queryKey: queryKeys.stats }),
  });
  const hideRow = (id: string) =>
    setHiddenIds((prev) => {
      const next = new Set(prev);
      next.add(id);
      return next;
    });

  const dreamMutation = useMutation({
    mutationFn: api.dream,
    onSuccess: (res) => {
      setDreamResult(res);
      toast(t('settings.dreamCycle'), 'success');
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  const consolidateMutation = useMutation({
    mutationFn: api.consolidate,
    onSuccess: (res) => {
      setConsolResult(res);
      toast(t('settings.consolidation'), 'success');
      qc.invalidateQueries({ queryKey: queryKeys.stats });
      qc.invalidateQueries({ queryKey: queryKeys.retentionDistribution });
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  const reflectMutation = useMutation({
    mutationFn: () => api.reflect(undefined, 'standard'),
    onSuccess: (res) => {
      setReflectResult(res);
      toast(t('settings.reflectDone'), 'success');
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  const confidenceMutation = useMutation({
    mutationFn: () => api.confidence('audit'),
    onSuccess: (res) => {
      setConfidenceResult(res);
      toast(t('settings.confidenceDone'), 'success');
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <h2 className="text-lg font-bold text-foreground">{t('settings.title')}</h2>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.system')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-3 text-xs">
            <div>
              <span className="text-muted-foreground">{t('settings.connection')}</span>
              <div className={connected ? 'text-emerald-500' : 'text-red-500'}>
                {connected ? t('status.connected') : t('status.disconnected')}
              </div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.version')}</span>
              <div className="text-foreground">{health?.version ?? '...'}</div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.memoriesTotal')}</span>
              <div className="text-foreground tabular-nums">{memoryCount || stats?.totalMemories || '...'}</div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.avgRetention')}</span>
              <div className="text-foreground tabular-nums">
                {((avgRetention || stats?.averageRetention || 0) * 100).toFixed(1)}%
              </div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.embeddingModel')}</span>
              <div className="text-foreground text-xs">{stats?.embeddingModel || 'nomic-embed-text-v1.5'} (384D)</div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.reranker')}</span>
              <div className="text-foreground text-xs">Jina Reranker v2 Base</div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.embeddingCoverage')}</span>
              <div className="text-foreground tabular-nums">
                {stats
                  ? `${stats.withEmbeddings} / ${stats.totalMemories} (${stats.embeddingCoverage.toFixed(0)}%)`
                  : '...'}
              </div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.scheduler')}</span>
              <div className="text-foreground text-xs">FSRS-6 (21 parameters)</div>
            </div>
          </div>
        </CardContent>
      </Card>

      <SectionErrorBoundary fallbackTitle={t('settings.operations')}>
        <Card>
          <CardHeader>
            <CardTitle>{t('settings.operations')}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-foreground font-medium">{t('settings.dreamCycle')}</div>
                <div className="text-xs text-muted-foreground mt-0.5">{t('settings.dreamDesc')}</div>
              </div>
              <Button
                variant="dream"
                size="sm"
                onClick={() => dreamMutation.mutate()}
                disabled={dreamMutation.isPending}
              >
                {dreamMutation.isPending ? t('settings.dreaming') : t('settings.dream')}
              </Button>
            </div>
            {dreamResult && <DreamResultPanel result={dreamResult} />}

            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-foreground font-medium">{t('settings.consolidation')}</div>
                <div className="text-xs text-muted-foreground mt-0.5">{t('settings.consolidationDesc')}</div>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => consolidateMutation.mutate()}
                disabled={consolidateMutation.isPending}
              >
                {consolidateMutation.isPending ? t('settings.consolidating') : t('settings.consolidate')}
              </Button>
            </div>
            {consolResult && (
              <Card className="grid grid-cols-2 gap-2 text-xs">
                <div>
                  {t('settings.processed')}:{' '}
                  <span className="text-foreground tabular-nums">{consolResult.nodesProcessed}</span>
                </div>
                <div>
                  {t('settings.decay')}:{' '}
                  <span className="text-foreground tabular-nums">{consolResult.decayApplied}</span>
                </div>
                <div>
                  {t('settings.embeddings')}:{' '}
                  <span className="text-foreground tabular-nums">{consolResult.embeddingsGenerated}</span>
                </div>
                <div>
                  {t('settings.duration')}:{' '}
                  <span className="text-foreground tabular-nums">{consolResult.durationMs}ms</span>
                </div>
              </Card>
            )}
          </CardContent>
        </Card>
      </SectionErrorBoundary>

      <SectionErrorBoundary fallbackTitle={t('settings.metacognitive')}>
        <Card>
          <CardHeader>
            <CardTitle>{t('settings.metacognitive')}</CardTitle>
            <CardDescription>{t('settings.metacognitiveDesc')}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-foreground font-medium">{t('settings.reflect')}</div>
                <div className="text-xs text-muted-foreground mt-0.5">{t('settings.reflectDesc')}</div>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => reflectMutation.mutate()}
                disabled={reflectMutation.isPending}
              >
                {reflectMutation.isPending ? t('settings.reflecting') : t('settings.reflectBtn')}
              </Button>
            </div>
            {reflectResult && (
              <Card className="space-y-2 text-xs">
                <div className="text-foreground font-medium">{reflectResult.summary}</div>
                {reflectResult.structuredInsights?.map((ins) => (
                  <div
                    key={`${ins.type}|${ins.severity}|${ins.description}`}
                    className="flex gap-2 items-start border-t border-border pt-2"
                  >
                    <Badge
                      variant={ins.severity === 'high' ? 'danger' : ins.severity === 'medium' ? 'warning' : 'default'}
                    >
                      {ins.type}
                    </Badge>
                    <span className="text-muted-foreground flex-1">{ins.description}</span>
                  </div>
                ))}
              </Card>
            )}

            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-foreground font-medium">{t('settings.confidenceAudit')}</div>
                <div className="text-xs text-muted-foreground mt-0.5">{t('settings.confidenceDesc')}</div>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => confidenceMutation.mutate()}
                disabled={confidenceMutation.isPending}
              >
                {confidenceMutation.isPending ? t('settings.auditing') : t('settings.auditBtn')}
              </Button>
            </div>
            {confidenceResult?.results && confidenceResult.results.length > 0 && (
              <Card className="space-y-2 text-xs">
                <p className="text-muted-foreground">{t('settings.doubtModeHint')}</p>
                {confidenceResult.results
                  .filter((item) => !hiddenIds.has(item.id))
                  .slice(0, 5)
                  .map((item) => (
                    <div
                      key={item.id}
                      className="flex flex-col sm:flex-row sm:justify-between sm:items-start gap-2 border-b border-border pb-2 last:border-0"
                    >
                      <div className="flex-1 min-w-0">
                        <p className="text-muted-foreground line-clamp-2 break-words">{item.content}</p>
                        <div className="flex items-center gap-2 mt-1 flex-wrap">
                          <span
                            className={`tabular-nums text-[11px] ${item.confidence > 0.7 ? 'text-emerald-500' : item.confidence > 0.4 ? 'text-amber-500' : 'text-red-500'}`}
                          >
                            {(item.confidence * 100).toFixed(0)}%
                          </span>
                          <span className="text-muted-foreground text-[11px]">{item.classification}</span>
                        </div>
                      </div>
                      <div className="flex gap-1 flex-shrink-0 self-end sm:self-start">
                        <Button
                          variant="success"
                          size="sm"
                          className="text-[11px] py-1"
                          onClick={() => {
                            doubtPromote.mutate(item.id);
                            hideRow(item.id);
                          }}
                          disabled={doubtPromote.isPending}
                          aria-label={t('settings.doubtModeVerify')}
                        >
                          ✓ {t('settings.doubtModeVerify')}
                        </Button>
                        <Button
                          variant="danger"
                          size="sm"
                          className="text-[11px] py-1"
                          onClick={() => {
                            doubtDemote.mutate(item.id);
                            hideRow(item.id);
                          }}
                          disabled={doubtDemote.isPending}
                          aria-label={t('settings.doubtModeDemote')}
                        >
                          ↓ {t('settings.doubtModeDemote')}
                        </Button>
                      </div>
                    </div>
                  ))}
              </Card>
            )}
          </CardContent>
        </Card>
      </SectionErrorBoundary>

      <SectionErrorBoundary fallbackTitle="Maintenance">
        <MaintenancePanel />
      </SectionErrorBoundary>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.pipeline')}</CardTitle>
        </CardHeader>
        <CardContent>
          <PipelineVisualizer />
        </CardContent>
      </Card>

      {distribution && <GovernancePanel distribution={distribution} />}

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.architecture')}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-xs text-muted-foreground leading-relaxed">
            {t('settings.architectureDesc', { count: 29 })}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
