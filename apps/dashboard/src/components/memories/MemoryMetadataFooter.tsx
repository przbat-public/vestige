import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { InfoTooltip } from '@/components/ui/info-tooltip';
import { formatDateTime } from '@/lib/format';
import type { Memory } from '@/types';
import { formatNextReview } from './memoryDetailUtils';

interface Props {
  memory: Memory;
}

/**
 * Decide which temporal-status badge (if any) to show next to the review
 * count. Surfaces `validUntil` so users can spot superseded facts at a
 * glance — previously the field was loaded from the backend but never
 * rendered in any UI.
 */
function temporalStatus(memory: Memory): { kind: 'expired' | 'expiring' | 'valid'; daysDelta: number } | null {
  if (!memory.validUntil) return null;
  const expiresAt = new Date(memory.validUntil).getTime();
  if (Number.isNaN(expiresAt)) return null;
  const daysDelta = Math.round((expiresAt - Date.now()) / 86_400_000);
  if (daysDelta < 0) return { kind: 'expired', daysDelta: -daysDelta };
  // 30-day soft window matches the temporal MCP tool's "expiring soon" cue.
  if (daysDelta <= 30) return { kind: 'expiring', daysDelta };
  return { kind: 'valid', daysDelta };
}

/** Reviews count, next-review badge, temporal validity, timestamps. */
export function MemoryMetadataFooter({ memory }: Props) {
  const { t, i18n } = useTranslation();
  const nextReview = formatNextReview(memory.nextReviewAt, i18n.language, t('memories.nextReviewDueNow'));
  const temporal = temporalStatus(memory);

  return (
    <>
      <div className="text-xs text-muted-foreground flex items-center gap-3 flex-wrap">
        <span className="inline-flex items-center gap-1">
          {t('memories.reviews')}: <span className="text-foreground tabular-nums">{memory.reviewCount ?? 0}</span>
          <InfoTooltip content={t('memories.reviewsTooltip')} />
        </span>
        {nextReview && (
          <Badge
            variant={nextReview.isOverdue ? 'warning' : 'secondary'}
            aria-label={t('memories.nextReviewAria', { when: nextReview.label })}
          >
            ⟳ {t('memories.nextReviewLabel')} {nextReview.label}
          </Badge>
        )}
        {temporal?.kind === 'expired' && (
          <Badge variant="warning" aria-label={t('memories.temporalExpiredAria', { days: temporal.daysDelta })}>
            ⌛ {t('memories.temporalExpired', { days: temporal.daysDelta })}
          </Badge>
        )}
        {temporal?.kind === 'expiring' && (
          <Badge variant="secondary" aria-label={t('memories.temporalExpiringAria', { days: temporal.daysDelta })}>
            ⌛ {t('memories.temporalExpiring', { days: temporal.daysDelta })}
          </Badge>
        )}
      </div>

      <div className="text-xs text-muted-foreground space-y-1">
        <div>
          {t('memories.created')}: {formatDateTime(memory.createdAt, i18n.language)}
        </div>
        <div>
          {t('memories.updated')}: {formatDateTime(memory.updatedAt, i18n.language)}
        </div>
        {memory.lastAccessedAt && (
          <div>
            {t('memories.accessed')}: {formatDateTime(memory.lastAccessedAt, i18n.language)}
          </div>
        )}
      </div>
    </>
  );
}
