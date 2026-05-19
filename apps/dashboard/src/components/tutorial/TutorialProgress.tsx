import { useTranslation } from 'react-i18next';
import { useExploredSections } from '@/stores/tutorial-progress';
import { useQuizResult } from '@/stores/tutorial-quiz';
import { TUTORIAL_SECTIONS } from './sections';

interface TutorialProgressProps {
  /** Restrict the denominator to the currently-visible mode. */
  visibleIds: ReadonlySet<string>;
}

/**
 * "5 / 12 sections explored" pill + completion badge.
 *
 * Two signals:
 *   - sections explored — incrementing localStorage counter (see store)
 *   - quiz completed perfect score — gives the user a stable "I'm done" cue
 *
 * Deliberately small and quiet. A loud progress bar would feel gamified;
 * Vestige is a reference tool, not a course.
 */
export function TutorialProgress({ visibleIds }: TutorialProgressProps) {
  const { t } = useTranslation();
  const explored = useExploredSections();
  const quiz = useQuizResult();

  const sections = TUTORIAL_SECTIONS.filter((s) => visibleIds.size === 0 || visibleIds.has(s.id));
  const total = sections.length;
  const done = sections.filter((s) => explored.has(s.id)).length;
  const pct = total === 0 ? 0 : (done / total) * 100;

  const completed = quiz && quiz.score === quiz.total && quiz.total > 0;

  return (
    <div className="inline-flex items-center gap-2 text-xs text-muted-foreground">
      <div className="relative w-24 h-1.5 rounded-full bg-muted overflow-hidden" aria-hidden="true">
        <div className="h-full bg-primary transition-all" style={{ width: `${pct}%` }} />
      </div>
      <span className="font-mono">
        {done} / {total}
      </span>
      <span>{t('tutorial.progress.sections')}</span>
      {completed && (
        <span
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 text-[10px] font-medium"
          title={t('tutorial.progress.completedTitle')}
        >
          <span aria-hidden="true">✓</span>
          {t('tutorial.progress.completed')}
        </span>
      )}
    </div>
  );
}
