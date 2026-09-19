import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { Button } from '@/components/ui/button';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import { EVENT, track } from '@/stores/telemetry';
import { markTourTaken } from '@/stores/tutorial-tour';

/**
 * Five-step guided tour. Opens in a focused modal rather than a full
 * page-takeover overlay because:
 *
 *   - Vestige's pages are dense; floating callouts would have to dodge
 *     fluid layout. A modal cuts the coordination problem.
 *   - The user is in tutorial mode already — they came here to read.
 *     A modal keeps the eye-line predictable.
 *   - "Open this page" buttons let them defect to the live page whenever
 *     a step hooks them. That's the actual point: get them off the
 *     tutorial and into the product.
 *
 * Closing the modal — by Esc, clicking the backdrop, finishing, or
 * skipping — marks the tour as taken so the CTA on the tutorial header
 * fades to a "Replay tour" link.
 */

interface TourStep {
  icon: string;
  titleKey: string;
  bodyKey: string;
  /** Route to navigate to when the user clicks the action button. */
  route: string;
  routeLabelKey: string;
}

const STEPS: readonly TourStep[] = [
  {
    icon: '🌅',
    titleKey: 'tutorial.tour.s1.title',
    bodyKey: 'tutorial.tour.s1.body',
    route: '/briefing',
    routeLabelKey: 'tutorial.tour.s1.cta',
  },
  {
    icon: '🌐',
    titleKey: 'tutorial.tour.s2.title',
    bodyKey: 'tutorial.tour.s2.body',
    route: '/graph',
    routeLabelKey: 'tutorial.tour.s2.cta',
  },
  {
    icon: '🗂️',
    titleKey: 'tutorial.tour.s3.title',
    bodyKey: 'tutorial.tour.s3.body',
    route: '/memories',
    routeLabelKey: 'tutorial.tour.s3.cta',
  },
  {
    icon: '🔁',
    titleKey: 'tutorial.tour.s4.title',
    bodyKey: 'tutorial.tour.s4.body',
    route: '/review',
    routeLabelKey: 'tutorial.tour.s4.cta',
  },
  {
    icon: '🛠️',
    titleKey: 'tutorial.tour.s5.title',
    bodyKey: 'tutorial.tour.s5.body',
    route: '/settings',
    routeLabelKey: 'tutorial.tour.s5.cta',
  },
];

interface InteractiveTourProps {
  open: boolean;
  onClose: () => void;
}

export function InteractiveTour({ open, onClose }: InteractiveTourProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [stepIdx, setStepIdx] = useState(0);
  // Modal contract: Tab stays inside the tour, and focus returns to the
  // "Start tour" button when it closes.
  const trapRef = useFocusTrap<HTMLDivElement>({ active: open });

  useEffect(() => {
    if (!open) return;
    setStepIdx(0);
    track(EVENT.tutorial_tour_start);
    // The Esc handler doesn't read `stepIdx` directly — it just closes the
    // tour. The Arrow handlers use the setter's callback form so we don't
    // need stepIdx in the deps either; this keeps the effect stable and
    // avoids tearing the listener down on every key press.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        track(EVENT.tutorial_tour_skip);
        markTourTaken();
        onClose();
      }
      if (e.key === 'ArrowRight') setStepIdx((i) => Math.min(STEPS.length - 1, i + 1));
      if (e.key === 'ArrowLeft') setStepIdx((i) => Math.max(0, i - 1));
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;
  const step = STEPS[stepIdx];

  const finish = (reason: 'complete' | 'skip') => {
    if (reason === 'complete') track(EVENT.tutorial_tour_complete);
    else track(EVENT.tutorial_tour_skip, { step: stepIdx });
    markTourTaken();
    onClose();
  };

  const openPage = () => {
    track(EVENT.tutorial_tour_complete, { route: step.route });
    markTourTaken();
    onClose();
    navigate(step.route);
  };

  return (
    <div
      ref={trapRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="tour-title"
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) finish('skip');
      }}
      onKeyDown={(e) => {
        // Backdrop has dialog semantics already; this swallows stray clicks
        // bubbling from inside the modal so Esc on focused buttons closes.
        if (e.key === 'Escape') finish('skip');
      }}
    >
      <div className="w-full max-w-md rounded-2xl border border-border bg-card shadow-2xl overflow-hidden">
        <div className="p-6 space-y-4">
          <div className="flex items-start justify-between gap-3">
            <div className="flex items-center gap-3">
              <span aria-hidden="true" className="text-3xl">
                {step.icon}
              </span>
              <h2 id="tour-title" className="text-base font-semibold text-foreground">
                {t(step.titleKey)}
              </h2>
            </div>
            <button
              type="button"
              onClick={() => finish('skip')}
              className="text-muted-foreground hover:text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-ring rounded-sm px-2 py-1"
              aria-label={t('tutorial.tour.skipAria')}
            >
              {t('tutorial.tour.skip')}
            </button>
          </div>
          <p className="text-sm text-muted-foreground leading-relaxed">{t(step.bodyKey)}</p>
        </div>
        <div className="px-6 py-3 bg-muted/30 border-t border-border flex items-center justify-between gap-3">
          <div className="flex gap-1.5" role="tablist" aria-label={t('tutorial.tour.dotsAria')}>
            {STEPS.map((_, idx) => (
              <button
                key={STEPS[idx].titleKey}
                type="button"
                role="tab"
                aria-selected={idx === stepIdx}
                aria-label={t('tutorial.tour.dotAria', { current: idx + 1, total: STEPS.length })}
                onClick={() => setStepIdx(idx)}
                className={`h-1.5 rounded-full transition-all ${
                  idx === stepIdx ? 'w-6 bg-primary' : 'w-1.5 bg-muted-foreground/30 hover:bg-muted-foreground/50'
                }`}
              />
            ))}
          </div>
          <div className="flex items-center gap-2">
            {stepIdx > 0 && (
              <Button variant="ghost" size="sm" onClick={() => setStepIdx((i) => i - 1)}>
                {t('tutorial.tour.back')}
              </Button>
            )}
            <Button variant="ghost" size="sm" onClick={openPage}>
              {t(step.routeLabelKey)}
            </Button>
            {stepIdx < STEPS.length - 1 ? (
              <Button size="sm" onClick={() => setStepIdx((i) => i + 1)}>
                {t('tutorial.tour.next')}
              </Button>
            ) : (
              <Button size="sm" onClick={() => finish('complete')}>
                {t('tutorial.tour.finish')}
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
