import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DoubtList } from '@/components/DoubtList';
import { DreamResultPanel } from '@/components/DreamResultPanel';
import { InsightSourcesPanel } from '@/components/InsightSourcesPanel';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { api } from '@/stores/api';
import { toast } from '@/stores/toast';
import type { DreamResult, ReflectInsight } from '@/types';

/**
 * Morning briefing — what your memory engine wants you to look at today.
 *
 * Three lenses:
 * 1. Reflection (auto, cheap):  contradictions, knowledge gaps, stale decisions.
 * 2. Doubt mode (auto, cheap):  least-confident memories with Verify/Demote.
 * 3. Dream (on-demand, slow):   click to discover cross-domain connections;
 *    the result is rendered via the existing DreamResultPanel.
 *
 * Reflection and confidence audits are issued on mount because they are
 * idempotent reads (mostly). Dream is gated on a button press because it
 * actually mutates the connection graph and can take a few seconds.
 */
export function BriefingPage() {
  const { t } = useTranslation();
  const [dreamResult, setDreamResult] = useState<DreamResult | null>(null);

  const reflect = useQuery({
    queryKey: ['briefing', 'reflect', 'standard'],
    queryFn: () => api.reflect(undefined, 'standard'),
    staleTime: 5 * 60_000,
  });

  const confidence = useQuery({
    queryKey: ['briefing', 'confidence', 'audit'],
    queryFn: () => api.confidence('audit', undefined, 10),
    staleTime: 5 * 60_000,
  });

  const dreamMutation = useMutation({
    mutationFn: api.dream,
    onSuccess: (res) => {
      setDreamResult(res);
      toast(t('briefing.dreamCompletedToast'), 'success');
    },
    onError: (err) => toast(err instanceof Error ? err.message : t('common.error'), 'error'),
  });

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <header className="space-y-1">
        <h2 className="text-lg font-bold text-foreground">{t('briefing.title')}</h2>
        <p className="text-sm text-muted-foreground">{t('briefing.subtitle')}</p>
      </header>

      {/* Reflection */}
      <Card>
        <CardHeader>
          <CardTitle>{t('briefing.reflectionTitle')}</CardTitle>
          <CardDescription>{t('briefing.reflectionDesc')}</CardDescription>
        </CardHeader>
        <CardContent>
          {reflect.isLoading && <LoadingSpinner label={t('common.loading')} />}
          {reflect.isError && <QueryErrorPanel error={reflect.error} onRetry={reflect.refetch} />}
          {reflect.data && (
            <div className="space-y-3 text-xs">
              <p className="text-foreground font-medium">{reflect.data.summary}</p>
              {reflect.data.structuredInsights && reflect.data.structuredInsights.length > 0 ? (
                <ul className="space-y-3">
                  {reflect.data.structuredInsights.slice(0, 6).map((ins) => (
                    <ReflectionInsightItem key={`${ins.type}|${ins.severity}|${ins.description}`} insight={ins} />
                  ))}
                </ul>
              ) : (
                <p className="text-muted-foreground italic">{t('briefing.reflectionEmpty')}</p>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Doubt mode */}
      <Card>
        <CardHeader>
          <CardTitle>{t('briefing.doubtTitle')}</CardTitle>
          <CardDescription>{t('briefing.doubtDesc')}</CardDescription>
        </CardHeader>
        <CardContent>
          {confidence.isLoading && <LoadingSpinner label={t('common.loading')} />}
          {confidence.isError && <QueryErrorPanel error={confidence.error} onRetry={confidence.refetch} />}
          {confidence.data?.results && confidence.data.results.length > 0 ? (
            <DoubtList results={confidence.data.results} limit={5} />
          ) : confidence.data ? (
            <p className="text-xs text-muted-foreground italic">{t('briefing.doubtEmpty')}</p>
          ) : null}
        </CardContent>
      </Card>

      {/* Dream */}
      <Card>
        <CardHeader>
          <CardTitle>{t('briefing.dreamTitle')}</CardTitle>
          <CardDescription>{t('briefing.dreamDesc')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <p className="text-xs text-muted-foreground flex-1">{t('briefing.dreamHint')}</p>
            <Button variant="dream" size="sm" onClick={() => dreamMutation.mutate()} disabled={dreamMutation.isPending}>
              {dreamMutation.isPending ? t('common.loading') : t('briefing.dreamRun')}
            </Button>
          </div>
          {dreamResult && <DreamResultPanel result={dreamResult} />}
        </CardContent>
      </Card>
    </div>
  );
}

interface ReflectionInsightItemProps {
  insight: ReflectInsight;
}

/**
 * One reflection insight + its sources. The sources panel is the
 * "why should I trust this" affordance — instead of a one-line claim
 * ("3 contradictions found") the user can expand to see the actual
 * memories that produced the contradiction.
 */
function ReflectionInsightItem({ insight }: ReflectionInsightItemProps) {
  const { t } = useTranslation();
  const variant = insight.severity === 'high' ? 'danger' : insight.severity === 'medium' ? 'warning' : 'default';

  return (
    <li className="space-y-2 border-t border-border pt-3">
      <div className="flex gap-2 items-start">
        <Badge variant={variant} className="shrink-0">
          {t(`briefing.insightType.${insight.type}`, { defaultValue: insight.type })}
        </Badge>
        <span className="text-muted-foreground flex-1">{insight.description}</span>
      </div>
      {insight.suggestion && <p className="text-[10px] text-muted-foreground/80 italic pl-1">{insight.suggestion}</p>}
      {insight.tags && insight.tags.length > 0 && (
        <div className="flex flex-wrap gap-1 pl-1">
          {insight.tags.map((tag) => (
            <Badge key={tag} variant="secondary" className="text-[10px]">
              {tag}
            </Badge>
          ))}
        </div>
      )}
      <InsightSourcesPanel memoryIds={insight.sourceMemoryIds} initialLimit={3} />
    </li>
  );
}
