import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DreamResultPanel } from '@/components/DreamResultPanel';
import { GovernancePanel } from '@/components/GovernancePanel';
import { PipelineVisualizer } from '@/components/PipelineVisualizer';
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { toast } from '@/stores/toast';
import { useWebSocket } from '@/stores/websocket';
import type { ConfidenceResult, ConsolidationResult, DreamResult, ReflectResult } from '@/types';

export function SettingsPage() {
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

  const dreamMutation = useMutation({
    mutationFn: api.dream,
    onSuccess: (res) => {
      setDreamResult(res);
      toast(t('settings.dreamCycle'), 'success');
    },
  });

  const consolidateMutation = useMutation({
    mutationFn: api.consolidate,
    onSuccess: (res) => {
      setConsolResult(res);
      toast(t('settings.consolidation'), 'success');
      qc.invalidateQueries({ queryKey: queryKeys.stats });
      qc.invalidateQueries({ queryKey: queryKeys.retentionDistribution });
    },
  });

  const reflectMutation = useMutation({
    mutationFn: () => api.reflect(undefined, 'standard'),
    onSuccess: (res) => {
      setReflectResult(res);
      toast(t('settings.reflectDone'), 'success');
    },
  });

  const confidenceMutation = useMutation({
    mutationFn: () => api.confidence('audit'),
    onSuccess: (res) => {
      setConfidenceResult(res);
      toast(t('settings.confidenceDone'), 'success');
    },
  });

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <h2 className="text-lg font-bold text-foreground">{t('settings.title')}</h2>

      {/* System Information */}
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
              <div className="text-foreground tabular-nums">{((avgRetention || stats?.averageRetention || 0) * 100).toFixed(1)}%</div>
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
                {stats ? `${stats.withEmbeddings} / ${stats.totalMemories} (${stats.embeddingCoverage.toFixed(0)}%)` : '...'}
              </div>
            </div>
            <div>
              <span className="text-muted-foreground">{t('settings.scheduler')}</span>
              <div className="text-foreground text-xs">FSRS-6 (21 parameters)</div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Operations */}
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.operations')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Dream Cycle */}
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

          {/* Consolidation */}
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
              <div>{t('settings.processed')}: <span className="text-foreground tabular-nums">{consolResult.nodesProcessed}</span></div>
              <div>{t('settings.decay')}: <span className="text-foreground tabular-nums">{consolResult.decayApplied}</span></div>
              <div>{t('settings.embeddings')}: <span className="text-foreground tabular-nums">{consolResult.embeddingsGenerated}</span></div>
              <div>{t('settings.duration')}: <span className="text-foreground tabular-nums">{consolResult.durationMs}ms</span></div>
            </Card>
          )}
        </CardContent>
      </Card>

      {/* Metacognitive Tools */}
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.metacognitive')}</CardTitle>
          <CardDescription>{t('settings.metacognitiveDesc')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Reflect */}
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
              {reflectResult.insights?.map((ins, i) => (
                <div key={`${ins.type}-${i}`} className="flex gap-2 items-start border-t border-border pt-2">
                  <span className={`px-1.5 py-0.5 rounded text-xs font-medium ${
                    ins.severity === 'high' ? 'bg-red-500/15 text-red-500' :
                    ins.severity === 'medium' ? 'bg-amber-500/15 text-amber-500' :
                    'bg-primary/10 text-primary'
                  }`}>{ins.type}</span>
                  <span className="text-muted-foreground flex-1">{ins.description}</span>
                </div>
              ))}
            </Card>
          )}

          {/* Confidence Audit */}
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
              {confidenceResult.results.slice(0, 5).map((item) => (
                <div key={item.id} className="flex justify-between items-start border-b border-border pb-2 last:border-0">
                  <span className="text-muted-foreground flex-1 line-clamp-1 mr-2">{item.content}</span>
                  <div className="flex gap-2 flex-shrink-0">
                    <span className={`tabular-nums ${item.confidence > 0.7 ? 'text-emerald-500' : item.confidence > 0.4 ? 'text-amber-500' : 'text-red-500'}`}>
                      {(item.confidence * 100).toFixed(0)}%
                    </span>
                    <span className="text-muted-foreground">{item.classification}</span>
                  </div>
                </div>
              ))}
            </Card>
          )}
        </CardContent>
      </Card>

      {/* Search Pipeline */}
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.pipeline')}</CardTitle>
        </CardHeader>
        <CardContent>
          <PipelineVisualizer />
        </CardContent>
      </Card>

      {distribution && <GovernancePanel distribution={distribution} />}

      {/* Architecture */}
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
