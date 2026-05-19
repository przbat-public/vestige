import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { Badge } from '@/components/ui/badge';
import { InfoTooltip } from '@/components/ui/info-tooltip';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { toast } from '@/stores/toast';
import type { Memory } from '@/types';

interface MemoryTemporalPanelProps {
  memory: Memory;
}

type ValidityKind = 'expired' | 'expiring' | 'valid';

interface ValidityState {
  kind: ValidityKind | null;
  days: number;
}

/**
 * Per-memory temporal-validity panel rendered inside `MemoryDetail`.
 *
 * The dashboard surfaces `validFrom` / `validUntil` only in the global
 * Temporal console today — power users had no way to act on validity
 * data from the memory drawer itself. This panel closes that gap with
 * three pieces:
 *
 * 1. A read-only summary of the validity window (dates + a relative
 *    "expired 12d ago" / "valid for another 47d" cue).
 * 2. A "Mark as superseded" mutation that wraps `temporal.invalidate` —
 *    cheaper than opening the Temporal page just to retire one memory.
 * 3. A shortcut to inspect the temporal evolution of the first tag,
 *    bridging the per-memory and per-topic views without duplicating
 *    Timeline / Temporal UI.
 *
 * The panel always renders so power users can demote stale facts even
 * when no window has been set yet — that's the most common case for
 * older memories ingested before temporal anchoring shipped.
 */
export function MemoryTemporalPanel({ memory }: MemoryTemporalPanelProps) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const navigate = useNavigate();

  const validity = useMemo<ValidityState>(() => computeValidity(memory), [memory]);

  const invalidateMutation = useMutation({
    mutationFn: () => api.temporal('invalidate', { memoryId: memory.id }),
    onSuccess: () => {
      toast(t('memories.temporalPanel.invalidated'), 'success');
      // Three caches need a refresh: the global temporal lists, the
      // memory list (validUntil flips), and this memory's individual
      // record so the drawer re-renders with the new window.
      qc.invalidateQueries({ queryKey: ['temporal'] });
      qc.invalidateQueries({ queryKey: queryKeys.memories() });
      qc.invalidateQueries({ queryKey: queryKeys.memory(memory.id) });
    },
    onError: (err: Error) => toast(err.message || t('common.error'), 'error'),
  });

  const firstTag = memory.tags[0];
  const badge = renderBadge(validity, t);
  const alreadyExpired = validity.kind === 'expired';

  return (
    <section
      aria-labelledby={`temporal-panel-${memory.id}`}
      className="space-y-2 rounded-lg border border-border bg-muted/30 p-3"
    >
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h3
          id={`temporal-panel-${memory.id}`}
          className="text-xs font-medium text-foreground inline-flex items-center gap-1"
        >
          {t('memories.temporalPanel.title')}
          <InfoTooltip
            content={t(
              'memories.temporalPanel.titleTooltip',
              'Temporal validity expresses when this memory is considered current. Marking it superseded removes it from "current" results without deleting the history.',
            )}
          />
        </h3>
        {badge && <Badge variant={badge.variant}>{badge.label}</Badge>}
      </div>

      {memory.validFrom || memory.validUntil ? (
        <dl className="text-[11px] text-muted-foreground space-y-0.5">
          {memory.validFrom && (
            <div className="flex gap-2">
              <dt className="shrink-0 w-20">{t('memories.temporalPanel.validFromLabel')}</dt>
              <dd className="tabular-nums text-foreground">{new Date(memory.validFrom).toLocaleString()}</dd>
            </div>
          )}
          {memory.validUntil && (
            <div className="flex gap-2">
              <dt className="shrink-0 w-20">{t('memories.temporalPanel.validUntilLabel')}</dt>
              <dd className="tabular-nums text-foreground">{new Date(memory.validUntil).toLocaleString()}</dd>
            </div>
          )}
        </dl>
      ) : (
        <p className="text-[11px] text-muted-foreground">
          {t('memories.temporalPanel.noWindow')}{' '}
          <span className="text-muted-foreground/70">{t('memories.temporalPanel.noWindowHint')}</span>
        </p>
      )}

      <div className="flex flex-wrap items-center gap-3 pt-1">
        <button
          type="button"
          onClick={() => invalidateMutation.mutate()}
          disabled={invalidateMutation.isPending || alreadyExpired}
          className="text-[11px] text-amber-600 dark:text-amber-400 hover:underline disabled:opacity-50 disabled:no-underline"
        >
          {invalidateMutation.isPending
            ? t('memories.temporalPanel.invalidating')
            : t('memories.temporalPanel.invalidate')}
        </button>

        {firstTag && (
          <button
            type="button"
            onClick={() => navigate(`/temporal?topic=${encodeURIComponent(firstTag)}&action=history`)}
            className="text-[11px] text-primary hover:underline"
            title={t('memories.temporalPanel.exploreTopicHint')}
          >
            {t('memories.temporalPanel.exploreTopic', { tag: firstTag })}
          </button>
        )}
      </div>
    </section>
  );
}

/**
 * Pure validity classifier — `kind === null` means "no `validUntil`
 * stored, nothing temporal to surface". 30-day soft window mirrors
 * `MemoryMetadataFooter` so the two badges agree.
 */
function computeValidity(memory: Memory): ValidityState {
  if (!memory.validUntil) return { kind: null, days: 0 };
  const until = new Date(memory.validUntil).getTime();
  if (Number.isNaN(until)) return { kind: null, days: 0 };
  const days = Math.round((until - Date.now()) / 86_400_000);
  if (days < 0) return { kind: 'expired', days: Math.abs(days) };
  if (days <= 30) return { kind: 'expiring', days };
  return { kind: 'valid', days };
}

function renderBadge(
  validity: ValidityState,
  t: ReturnType<typeof useTranslation>['t'],
): { variant: 'success' | 'warning' | 'danger'; label: string } | null {
  switch (validity.kind) {
    case 'expired':
      return {
        variant: 'danger',
        label: t('memories.temporalPanel.expiredRelative', { days: validity.days }),
      };
    case 'expiring':
      return {
        variant: 'warning',
        label: t('memories.temporalPanel.expiringRelative', { days: validity.days }),
      };
    case 'valid':
      return {
        variant: 'success',
        label: t('memories.temporalPanel.validRelative', { days: validity.days }),
      };
    default:
      return null;
  }
}
