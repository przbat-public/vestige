import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import type { Decision } from '@/types';

/**
 * Render one structured decision as a comparison matrix:
 *
 *   ┌────────────────────────┬─────────┬─────────┬─────────┐
 *   │ Question (recommended) │ choice₁ │ choice₂ │ choice₃ │
 *   ├────────────────────────┼─────────┼─────────┼─────────┤
 *   │ Criterion · weight     │  4.5    │  3.0    │   —     │
 *   └────────────────────────┴─────────┴─────────┴─────────┘
 *
 * Sparse score cells (the agent didn't score that pair) render as `—`,
 * not 0 — silent zeros would falsely penalise a choice the agent never
 * evaluated on that axis.
 *
 * Validity:
 *   * `expired === true` → red "Expired" badge (server-computed against
 *     wall-clock `now`).
 *   * `valid_until` in the future → muted "Valid until …" subtitle.
 *
 * The `<table>` is wrapped in a horizontally scrollable container so
 * very wide matrices (>4 choices on mobile) don't blow out the layout.
 */
interface DecisionCardProps {
  decision: Decision;
}

export function DecisionCard({ decision }: DecisionCardProps) {
  const { t, i18n } = useTranslation();

  // Index score cells by `criterionId|choiceId` for O(1) lookup during
  // table render. The matrix is sparse so a row × column loop with
  // `.find()` per cell would be O(n²·m) in the worst case.
  const scoreLookup = new Map<string, number>();
  for (const cell of decision.scoreMatrix) {
    scoreLookup.set(`${cell.criterionId}|${cell.choiceId}`, cell.score);
  }

  const chosen = decision.choices.find((c) => c.chosen);
  const createdAt = new Date(decision.createdAt);
  const validUntil = decision.validUntil ? new Date(decision.validUntil) : null;
  const locale = i18n.language;

  return (
    <Card className="space-y-3">
      <div className="space-y-1">
        <div className="flex items-start justify-between gap-3 flex-wrap">
          <h3 className="text-sm font-semibold text-foreground break-words">{decision.question}</h3>
          <div className="flex items-center gap-1.5 shrink-0">
            {decision.expired && <Badge variant="danger">{t('decisions.badge.expired')}</Badge>}
            {!decision.expired && validUntil && (
              <Badge variant="outline" className="text-muted-foreground">
                {t('decisions.badge.validUntil', { date: validUntil.toLocaleDateString(locale) })}
              </Badge>
            )}
            {decision.supersedes.length > 0 && (
              <Badge variant="secondary">
                {t('decisions.badge.supersedes', { count: decision.supersedes.length })}
              </Badge>
            )}
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          {t('decisions.subtitle', {
            date: createdAt.toLocaleDateString(locale),
            choice: chosen?.label ?? t('decisions.noRecommendation'),
          })}
        </p>
        {decision.rationale && <p className="text-xs text-foreground/80 italic break-words">{decision.rationale}</p>}
      </div>

      <div className="overflow-x-auto -mx-1 px-1">
        <table className="w-full text-xs">
          <caption className="sr-only">{t('decisions.tableCaption', { question: decision.question })}</caption>
          <thead>
            <tr className="border-b border-border">
              <th scope="col" className="text-left py-1.5 pr-2 font-medium text-muted-foreground whitespace-nowrap">
                {t('decisions.criterion')}
              </th>
              {decision.choices.map((choice) => (
                <th
                  key={choice.id}
                  scope="col"
                  className={`text-center py-1.5 px-2 font-medium whitespace-nowrap ${
                    choice.chosen ? 'text-primary' : 'text-muted-foreground'
                  }`}
                >
                  <div className="flex flex-col items-center gap-0.5">
                    <span>{choice.label}</span>
                    {choice.chosen && (
                      <span className="text-[10px] uppercase tracking-wide opacity-80">
                        {t('decisions.recommended')}
                      </span>
                    )}
                  </div>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {decision.criteria.map((criterion) => (
              <tr key={criterion.id} className="border-b border-border/40 last:border-0">
                <th scope="row" className="text-left py-1.5 pr-2 font-normal text-foreground/90 align-top">
                  <div className="flex flex-col">
                    <span>{criterion.label}</span>
                    <span className="text-[10px] text-muted-foreground tabular-nums">
                      {t('decisions.weight', { value: criterion.weight.toFixed(1) })}
                    </span>
                  </div>
                </th>
                {decision.choices.map((choice) => {
                  const score = scoreLookup.get(`${criterion.id}|${choice.id}`);
                  return (
                    <td
                      key={choice.id}
                      className={`text-center py-1.5 px-2 tabular-nums ${
                        choice.chosen ? 'font-semibold text-primary' : 'text-foreground/80'
                      }`}
                    >
                      {score === undefined ? <span className="text-muted-foreground/60">—</span> : score.toFixed(1)}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {decision.choices.some((c) => c.summary) && (
        <ul className="space-y-1 text-xs">
          {decision.choices
            .filter((c) => c.summary)
            .map((choice) => (
              <li key={choice.id} className="flex gap-2">
                <span className={`shrink-0 font-medium ${choice.chosen ? 'text-primary' : 'text-muted-foreground'}`}>
                  {choice.label}:
                </span>
                <span className="text-foreground/80 break-words">{choice.summary}</span>
              </li>
            ))}
        </ul>
      )}

      {decision.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {decision.tags.map((tag) => (
            <Badge key={tag} variant="outline" className="text-[10px]">
              {tag}
            </Badge>
          ))}
        </div>
      )}
    </Card>
  );
}
