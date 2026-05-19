import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { ProgressBar } from '@/components/ui/progress-bar';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useTrackPageView } from '@/stores/telemetry';
import { toast } from '@/stores/toast';
import type { FsrsRating } from '@/types';
import { NODE_TYPE_COLORS, retentionColor } from '@/types';

interface RatingButtonSpec {
  rating: FsrsRating;
  shortcut: '1' | '2' | '3' | '4';
  labelKey: string;
  hintKey: string;
  variant: 'danger' | 'secondary' | 'success' | 'default';
}

const RATING_BUTTONS: RatingButtonSpec[] = [
  { rating: 1, shortcut: '1', labelKey: 'review.again', hintKey: 'review.againHint', variant: 'danger' },
  { rating: 2, shortcut: '2', labelKey: 'review.hard', hintKey: 'review.hardHint', variant: 'secondary' },
  { rating: 3, shortcut: '3', labelKey: 'review.good', hintKey: 'review.goodHint', variant: 'success' },
  { rating: 4, shortcut: '4', labelKey: 'review.easy', hintKey: 'review.easyHint', variant: 'default' },
];

/**
 * FSRS-6 spaced-repetition review session.
 *
 * Pulls the queue once on mount, then walks through cards locally without
 * re-fetching after each review (avoids a UI stutter and saves N requests).
 * Each rating triggers a single `POST /memories/{id}/review` and advances to
 * the next card. When the local queue is exhausted, we re-fetch to pick up
 * any memories whose `next_review` has come due in the meantime.
 *
 * Keyboard shortcuts (Anki convention): 1=Again, 2=Hard, 3=Good, 4=Easy,
 * Space=Show full content. The page is the only mode in the dashboard with
 * keyboard-driven progress, so we hint shortcuts on every button.
 */
export function ReviewPage() {
  useTrackPageView('review');
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [index, setIndex] = useState(0);
  const [reviewedCount, setReviewedCount] = useState(0);
  const [revealed, setRevealed] = useState(true);

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: ['review', 'queue', 50],
    queryFn: () => api.review.queue(50),
    staleTime: 0,
  });

  const review = useMutation({
    mutationFn: (input: { id: string; rating: FsrsRating }) => api.memories.review(input.id, input.rating),
    onSuccess: () => {
      setReviewedCount((c) => c + 1);
      setIndex((i) => i + 1);
      setRevealed(true);
      // Stats / due counts in the sidebar must reflect the new state.
      qc.invalidateQueries({ queryKey: queryKeys.stats });
    },
    onError: (err) => {
      toast(err instanceof Error ? err.message : t('common.error'), 'error');
    },
  });

  const queue = useMemo(() => data?.memories ?? [], [data]);
  const current = queue[index];

  const onRate = useCallback(
    (rating: FsrsRating) => {
      if (!current || review.isPending) return;
      review.mutate({ id: current.id, rating });
    },
    [current, review],
  );

  const refillQueue = useCallback(() => {
    setIndex(0);
    refetch();
  }, [refetch]);

  // Keyboard shortcuts. Bound at the page level so the user can rate even
  // without focusing a button. We ignore key events when an input has focus
  // so future search/filter UIs don't accidentally trigger a rating.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) {
        return;
      }
      if (e.key >= '1' && e.key <= '4') {
        const rating = Number(e.key) as FsrsRating;
        e.preventDefault();
        onRate(rating);
        return;
      }
      if (e.key === ' ') {
        e.preventDefault();
        setRevealed((r) => !r);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onRate]);

  if (isLoading) {
    return <LoadingSpinner label={t('common.loading')} className="h-full" />;
  }

  if (isError) {
    return <QueryErrorPanel className="m-4" error={error} onRetry={refetch} />;
  }

  if (queue.length === 0) {
    return (
      <EmptyState
        icon="✦"
        title={t('review.queueEmptyTitle')}
        description={t('review.queueEmptyHint')}
        className="h-full"
      />
    );
  }

  if (index >= queue.length) {
    return (
      <div className="flex flex-col items-center justify-center h-full px-4 text-center gap-3">
        <span className="text-5xl" aria-hidden="true">
          ☑︎
        </span>
        <h2 className="text-lg font-semibold text-foreground">{t('review.sessionDone')}</h2>
        <p className="text-sm text-muted-foreground max-w-md">
          {t('review.sessionDoneHint', { count: reviewedCount })}
        </p>
        <Button variant="default" onClick={refillQueue}>
          {t('review.refresh')}
        </Button>
      </div>
    );
  }

  const remaining = queue.length - index;

  return (
    <main className="h-full flex flex-col items-center px-4 py-6 gap-4" aria-label={t('review.title')}>
      <header className="w-full max-w-2xl flex items-center justify-between text-xs text-muted-foreground">
        <h2 className="text-sm font-semibold text-foreground">{t('review.title')}</h2>
        <div className="flex items-center gap-3 tabular-nums">
          <span>
            {t('review.progress', {
              done: reviewedCount,
              total: reviewedCount + remaining,
            })}
          </span>
        </div>
      </header>

      <ProgressBar
        value={reviewedCount}
        max={Math.max(1, reviewedCount + remaining)}
        label={t('review.progressLabel')}
        color="var(--color-primary)"
        showValue={false}
        className="w-full max-w-2xl"
      />

      <Card className="w-full max-w-2xl space-y-4 p-5 sm:p-6">
        <div className="flex items-center justify-between gap-2">
          <Badge color={NODE_TYPE_COLORS[current.nodeType]}>
            {t(`nodeTypes.${current.nodeType}`, { defaultValue: current.nodeType })}
          </Badge>
          <span
            role="img"
            className="text-xs tabular-nums"
            style={{ color: retentionColor(current.retentionStrength) }}
            aria-label={t('review.currentRetention', {
              value: (current.retentionStrength * 100).toFixed(0),
            })}
          >
            {(current.retentionStrength * 100).toFixed(0)}%
          </span>
        </div>

        {revealed ? (
          <p className="text-sm text-foreground leading-relaxed break-words whitespace-pre-wrap">{current.content}</p>
        ) : (
          <button
            type="button"
            onClick={() => setRevealed(true)}
            className="w-full rounded-lg border-2 border-dashed border-border py-12 text-sm text-muted-foreground hover:text-foreground hover:border-foreground/50 transition focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {t('review.tapToReveal')}
          </button>
        )}

        {current.tags.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {current.tags.map((tag) => (
              <Badge key={tag} variant="secondary">
                {tag}
              </Badge>
            ))}
          </div>
        )}

        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-2">
          {RATING_BUTTONS.map((spec) => (
            <Button
              key={spec.rating}
              variant={spec.variant}
              size="sm"
              onClick={() => onRate(spec.rating)}
              disabled={review.isPending}
              aria-keyshortcuts={spec.shortcut}
              className="flex flex-col items-center gap-0.5 py-3"
            >
              <span className="text-xs font-semibold">{t(spec.labelKey)}</span>
              <span className="text-[10px] opacity-70">{spec.shortcut}</span>
            </Button>
          ))}
        </div>

        <p className="text-[11px] text-muted-foreground text-center">{t('review.shortcutHint')}</p>
      </Card>
    </main>
  );
}
