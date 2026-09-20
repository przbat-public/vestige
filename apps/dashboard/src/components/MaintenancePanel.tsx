import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/stores/api';
import { confirm } from '@/stores/confirm';
import { queryKeys } from '@/stores/query';
import { toast } from '@/stores/toast';

type Awaited2<T> = T extends Promise<infer U> ? U : never;
type RegenResult = Awaited2<ReturnType<typeof api.maintenance.regenerateEmbeddings>>;
type DupResult = Awaited2<ReturnType<typeof api.maintenance.findDuplicates>>;
type GcResult = Awaited2<ReturnType<typeof api.maintenance.gc>>;
type BackupResult = Awaited2<ReturnType<typeof api.maintenance.backup>>;

/**
 * Settings → Maintenance section.
 *
 * Surfaces the operational MCP tools the user previously had to run from the
 * agent: regenerate-embeddings, find-duplicates, gc (dry-run), backup. Each
 * button triggers a single mutation and renders an inline summary. Destructive
 * paths (gc with dry_run=false) require explicit re-click after preview.
 *
 * Responsive: the buttons stack on narrow screens via flex-wrap.
 */
export function MaintenancePanel() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [regenResult, setRegenResult] = useState<RegenResult | null>(null);
  const [dupResult, setDupResult] = useState<DupResult | null>(null);
  const [gcResult, setGcResult] = useState<GcResult | null>(null);
  const [backupResult, setBackupResult] = useState<BackupResult | null>(null);

  const handleError = (key: string) => (err: unknown) => {
    const msg = err instanceof Error ? err.message : t(key);
    toast(msg, 'error');
  };

  const regen = useMutation({
    mutationFn: () => api.maintenance.regenerateEmbeddings({ force: false }),
    onSuccess: (res) => {
      setRegenResult(res);
      toast(
        t('maintenance.regenSuccess', {
          successful: res.successful,
          skipped: res.skipped,
        }),
        'success',
      );
      qc.invalidateQueries({ queryKey: queryKeys.stats });
    },
    onError: handleError('common.error'),
  });

  // 0.82 calibrated against the 2026-05-07 audit (1960-memory base):
  //   * 0.85+ — zero clusters returned, missed real duplicates
  //   * 0.82  — ~30 candidate clusters, ~20% true duplicates
  // Lower threshold surfaces splittable compounds and rephrasings, which is
  // the actual quality problem in long-running bases. UI surfaces an
  // explicit "review before merging" warning so users don't bulk-delete.
  const dup = useMutation({
    mutationFn: () => api.maintenance.findDuplicates({ similarity_threshold: 0.82, limit: 30 }),
    onSuccess: (res) => {
      setDupResult(res);
      const count = res.totalClusters ?? res.clusters?.length ?? 0;
      toast(t('maintenance.dupSuccess', { count }), 'success');
    },
    onError: handleError('common.error'),
  });

  const gcPreview = useMutation({
    mutationFn: () => api.maintenance.gc({ dry_run: true, min_retention: 0.1 }),
    onSuccess: (res) => {
      setGcResult(res);
      toast(t('maintenance.gcPreviewSuccess', { count: res.candidateCount }), 'success');
    },
    onError: handleError('common.error'),
  });

  const gcDelete = useMutation({
    // `confirmed: true` is not decoration: `tools::maintenance::execute_gc`
    // refuses `dry_run: false` without it, and the refusal used to surface as
    // an HTTP 500 — so the destructive button behind the themed confirm dialog
    // failed every single time. The user's confirmation *is* the
    // acknowledgement the MCP gate asks for.
    mutationFn: () => api.maintenance.gc({ dry_run: false, min_retention: 0.1, confirmed: true }),
    onSuccess: (res) => {
      setGcResult(res);
      toast(t('maintenance.gcDeleteSuccess', { count: res.deleted ?? 0 }), 'success');
      qc.invalidateQueries({ queryKey: queryKeys.stats });
      qc.invalidateQueries({ queryKey: ['memories'] });
      qc.invalidateQueries({ queryKey: ['graph'] });
    },
    onError: handleError('common.error'),
  });

  const backup = useMutation({
    mutationFn: () => api.maintenance.backup(),
    onSuccess: (res) => {
      setBackupResult(res);
      toast(t('maintenance.backupSuccess'), 'success');
    },
    onError: handleError('common.error'),
  });

  const onConfirmDelete = async () => {
    if (!gcResult || gcResult.candidateCount === 0) return;
    const ok = await confirm({
      message: t('maintenance.gcConfirm', { count: gcResult.candidateCount }),
      destructive: true,
      confirmLabel: t('common.delete'),
    });
    if (ok) gcDelete.mutate();
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('maintenance.title')}</CardTitle>
        <CardDescription>{t('maintenance.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Regenerate Embeddings */}
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex-1 min-w-[12rem]">
            <div className="text-sm text-foreground font-medium">{t('maintenance.regenTitle')}</div>
            <p className="text-xs text-muted-foreground mt-0.5">{t('maintenance.regenDesc')}</p>
          </div>
          <Button variant="outline" size="sm" onClick={() => regen.mutate()} disabled={regen.isPending}>
            {regen.isPending ? t('common.loading') : t('maintenance.regenBtn')}
          </Button>
        </div>
        {regenResult && (
          <div className="text-xs text-muted-foreground tabular-nums">
            {t('maintenance.regenResult', {
              successful: regenResult.successful,
              skipped: regenResult.skipped,
              failed: regenResult.failed,
              ms: regenResult.durationMs,
            })}
          </div>
        )}

        {/* Find Duplicates */}
        <div className="flex flex-wrap items-start justify-between gap-3 border-t border-border pt-4">
          <div className="flex-1 min-w-[12rem]">
            <div className="text-sm text-foreground font-medium">{t('maintenance.dupTitle')}</div>
            <p className="text-xs text-muted-foreground mt-0.5">{t('maintenance.dupDesc')}</p>
          </div>
          <Button variant="outline" size="sm" onClick={() => dup.mutate()} disabled={dup.isPending}>
            {dup.isPending ? t('common.loading') : t('maintenance.dupBtn')}
          </Button>
        </div>
        {dupResult && <DupResultPanel result={dupResult} />}

        {/* Garbage Collection */}
        <div className="flex flex-wrap items-start justify-between gap-3 border-t border-border pt-4">
          <div className="flex-1 min-w-[12rem]">
            <div className="text-sm text-foreground font-medium">{t('maintenance.gcTitle')}</div>
            <p className="text-xs text-muted-foreground mt-0.5">{t('maintenance.gcDesc')}</p>
          </div>
          <div className="flex gap-2 flex-wrap">
            <Button
              variant="outline"
              size="sm"
              onClick={() => gcPreview.mutate()}
              disabled={gcPreview.isPending || gcDelete.isPending}
            >
              {gcPreview.isPending ? t('common.loading') : t('maintenance.gcPreviewBtn')}
            </Button>
            {gcResult?.dryRun && gcResult.candidateCount > 0 && (
              <Button variant="danger" size="sm" onClick={onConfirmDelete} disabled={gcDelete.isPending}>
                {gcDelete.isPending
                  ? t('common.loading')
                  : t('maintenance.gcDeleteBtn', { count: gcResult.candidateCount })}
              </Button>
            )}
          </div>
        </div>
        {gcResult && (
          <div className="text-xs text-muted-foreground space-y-1">
            {gcResult.dryRun ? (
              <p className="tabular-nums">{t('maintenance.gcPreviewResult', { count: gcResult.candidateCount })}</p>
            ) : (
              <p className="tabular-nums">{t('maintenance.gcDeleteResult', { count: gcResult.deleted ?? 0 })}</p>
            )}
          </div>
        )}

        {/* Backup */}
        <div className="flex flex-wrap items-start justify-between gap-3 border-t border-border pt-4">
          <div className="flex-1 min-w-[12rem]">
            <div className="text-sm text-foreground font-medium">{t('maintenance.backupTitle')}</div>
            <p className="text-xs text-muted-foreground mt-0.5">{t('maintenance.backupDesc')}</p>
          </div>
          <Button variant="outline" size="sm" onClick={() => backup.mutate()} disabled={backup.isPending}>
            {backup.isPending ? t('common.loading') : t('maintenance.backupBtn')}
          </Button>
        </div>
        {backupResult && (
          <div className="text-xs text-muted-foreground break-words">
            {t('maintenance.backupResult', {
              path: backupResult.path,
              kb: Math.round(backupResult.sizeBytes / 1024),
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * Render the find-duplicates result. Extracted from `MaintenancePanel` to
 * keep that component below Biome's cognitive-complexity threshold and to
 * keep the duplicate review affordances co-located in one place.
 */
function DupResultPanel({ result }: { result: DupResult }) {
  const { t } = useTranslation();
  return (
    <div className="text-xs space-y-1.5">
      {result.warning && <p className="text-amber-500">{result.warning}</p>}
      <p className="text-muted-foreground tabular-nums">
        {t('maintenance.dupResult', {
          clusters: result.totalClusters,
          memories: result.totalWithEmbeddings,
        })}
      </p>
      {(result.totalClusters ?? 0) > 0 && (
        <p className="text-amber-500/90 italic">{t('maintenance.dupReviewWarning')}</p>
      )}
      {result.clusters?.slice(0, 3).map((c) => (
        <Card key={c.clusterId} className="text-xs space-y-1">
          <div className="flex items-center gap-2">
            <Badge variant="warning">{t('maintenance.dupClusterLabel', { size: c.size })}</Badge>
            <span className="text-muted-foreground">{c.suggestedAction}</span>
          </div>
          <ul className="space-y-0.5 ml-1">
            {c.members.slice(0, 3).map((m) => (
              <li key={m.id} className="text-muted-foreground line-clamp-1" title={m.contentPreview}>
                {m.contentPreview}
              </li>
            ))}
          </ul>
        </Card>
      ))}
    </div>
  );
}
