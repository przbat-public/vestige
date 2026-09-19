import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { InfoTooltip } from '@/components/ui/info-tooltip';
import { useMemoryMutations } from '@/hooks/useMemoryMutations';
import type { ConfidenceResult } from '@/types';

interface DoubtListProps {
  results: ConfidenceResult['results'];
  /** Cap how many results to render. Defaults to 5 — same default as the Settings preview. */
  limit?: number;
}

/**
 * Reusable Doubt-mode list: low-confidence memories with inline Verify/Demote.
 *
 * Locally hides rows after action so the user can sweep through doubts
 * without each click triggering a re-fetch. Settings page and the Briefing
 * page both render this — keeping the markup in one place avoids drift in
 * Verify-vs-Demote semantics.
 */
export function DoubtList({ results, limit = 5 }: DoubtListProps) {
  const { t } = useTranslation();
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const { promote, demote } = useMemoryMutations();

  const visible = (results ?? []).filter((r) => !hidden.has(r.id)).slice(0, limit);

  if (visible.length === 0) {
    return null;
  }

  const hide = (id: string) =>
    setHidden((prev) => {
      const next = new Set(prev);
      next.add(id);
      return next;
    });

  return (
    <div className="space-y-2 text-xs">
      <p className="text-muted-foreground inline-flex items-center gap-1.5">
        {t('settings.doubtModeHint')}
        <InfoTooltip ariaLabel={t('settings.doubtModeTooltipAria')} content={t('settings.doubtModeTooltip')} />
      </p>
      {visible.map((item) => (
        <div
          key={item.id}
          className="flex flex-col sm:flex-row sm:justify-between sm:items-start gap-2 border-b border-border pb-2 last:border-0"
        >
          <div className="flex-1 min-w-0">
            <p className="text-muted-foreground line-clamp-2 break-words">{item.content}</p>
            <div className="flex items-center gap-2 mt-1 flex-wrap">
              <span
                className={`tabular-nums text-[11px] ${
                  item.confidence > 0.7 ? 'text-emerald-500' : item.confidence > 0.4 ? 'text-amber-500' : 'text-red-500'
                }`}
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
                promote.mutate(item.id);
                hide(item.id);
              }}
              disabled={promote.isPending}
              aria-label={t('settings.doubtModeVerify')}
            >
              ✓ {t('settings.doubtModeVerify')}
            </Button>
            <Button
              variant="danger"
              size="sm"
              className="text-[11px] py-1"
              onClick={() => {
                demote.mutate(item.id);
                hide(item.id);
              }}
              disabled={demote.isPending}
              aria-label={t('settings.doubtModeDemote')}
            >
              ↓ {t('settings.doubtModeDemote')}
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}
