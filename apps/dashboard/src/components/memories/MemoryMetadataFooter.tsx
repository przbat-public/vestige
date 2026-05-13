import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import type { Memory } from '@/types';
import { formatNextReview } from './memoryDetailUtils';

interface Props {
  memory: Memory;
}

/** Reviews count, next-review badge, created/updated/accessed timestamps. */
export function MemoryMetadataFooter({ memory }: Props) {
  const { t, i18n } = useTranslation();
  const nextReview = formatNextReview(memory.nextReviewAt, i18n.language, t('memories.nextReviewDueNow'));

  return (
    <>
      <div className="text-xs text-muted-foreground flex items-center gap-3 flex-wrap">
        <span>
          {t('memories.reviews')}: <span className="text-foreground tabular-nums">{memory.reviewCount ?? 0}</span>
        </span>
        {nextReview && (
          <Badge
            variant={nextReview.isOverdue ? 'warning' : 'secondary'}
            aria-label={t('memories.nextReviewAria', { when: nextReview.label })}
          >
            ⟳ {t('memories.nextReviewLabel')} {nextReview.label}
          </Badge>
        )}
      </div>

      <div className="text-xs text-muted-foreground space-y-1">
        <div>
          {t('memories.created')}: {new Date(memory.createdAt).toLocaleString()}
        </div>
        <div>
          {t('memories.updated')}: {new Date(memory.updatedAt).toLocaleString()}
        </div>
        {memory.lastAccessedAt && (
          <div>
            {t('memories.accessed')}: {new Date(memory.lastAccessedAt).toLocaleString()}
          </div>
        )}
      </div>
    </>
  );
}
