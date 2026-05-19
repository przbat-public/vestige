import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import { InfoTooltip } from '@/components/ui/info-tooltip';
import type { Insight } from '@/types';

/**
 * Render one synthesised Insight Tier card.
 *
 * The dream cycle and `reflect` tool emit tentative observations
 * ("3 of your last 5 OAuth bugs were token-cache invalidation
 * issues"). Each one becomes a `KnowledgeNode` of type `"insight"`
 * with structured metadata under `extra_json.insight`. This card
 * surfaces them so the agent (or a human reviewer) can mark them
 * validated by calling `memory(action="promote")`.
 *
 * Visual cues:
 *   * Unvalidated → yellow "Unvalidated" badge, normal background.
 *   * Validated   → green "Validated" badge, subtle ring around the card.
 *   * Origin chip — distinguishes dream / reflect / synthesized at a glance.
 *
 * Confidence and novelty render as inline progress bars (0..1). We
 * round to one decimal because the underlying f32 noise is well
 * below display precision.
 */
interface InsightCardProps {
  insight: Insight;
}

export function InsightCard({ insight }: InsightCardProps) {
  const { t, i18n } = useTranslation();
  const createdAt = new Date(insight.createdAt);
  const validatedAt = insight.validatedAt ? new Date(insight.validatedAt) : null;
  const locale = i18n.language;

  const confidencePct = Math.round(insight.confidence * 100);
  const noveltyPct = Math.round(insight.novelty * 100);

  return (
    <Card className={insight.validated ? 'space-y-3 ring-1 ring-emerald-500/40' : 'space-y-3'}>
      <div className="space-y-1">
        <div className="flex items-start justify-between gap-3 flex-wrap">
          <p className="text-sm text-foreground break-words flex-1 min-w-0">{insight.content}</p>
          <div className="flex items-center gap-1.5 shrink-0">
            {insight.validated ? (
              <Badge variant="success">{t('insights.badge.validated', 'Validated')}</Badge>
            ) : (
              <Badge variant="outline" className="text-amber-600 border-amber-500/40">
                {t('insights.badge.unvalidated', 'Unvalidated')}
              </Badge>
            )}
            <Badge variant="secondary">{insight.insightType}</Badge>
            <Badge variant="outline" className="text-muted-foreground">
              {insight.origin}
            </Badge>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          {t('insights.subtitle', {
            date: createdAt.toLocaleDateString(locale),
            sources: insight.sourceMemoryIds.length,
            defaultValue: '{{date}} · derived from {{sources}} memories',
          })}
          {validatedAt && (
            <>
              {' · '}
              {t('insights.validatedOn', {
                date: validatedAt.toLocaleDateString(locale),
                defaultValue: 'validated {{date}}',
              })}
            </>
          )}
        </p>
      </div>

      <div className="grid grid-cols-2 gap-3 text-xs">
        <div>
          <div className="flex justify-between text-muted-foreground mb-0.5">
            <span className="inline-flex items-center gap-1">
              {t('insights.confidence', 'Confidence')}
              <InfoTooltip
                content={t(
                  'insights.confidenceTooltip',
                  'How sure the engine is that the pattern actually holds across the source memories. Driven by support count and contradiction signals.',
                )}
              />
            </span>
            <span>{confidencePct}%</span>
          </div>
          <div className="h-1.5 rounded bg-muted overflow-hidden">
            <div className="h-full bg-foreground/60" style={{ width: `${confidencePct}%` }} aria-hidden />
          </div>
        </div>
        <div>
          <div className="flex justify-between text-muted-foreground mb-0.5">
            <span className="inline-flex items-center gap-1">
              {t('insights.novelty', 'Novelty')}
              <InfoTooltip
                content={t(
                  'insights.noveltyTooltip',
                  'How different this insight is from what was already known. High novelty means the dream cycle stitched something the existing graph did not encode.',
                )}
              />
            </span>
            <span>{noveltyPct}%</span>
          </div>
          <div className="h-1.5 rounded bg-muted overflow-hidden">
            <div className="h-full bg-foreground/60" style={{ width: `${noveltyPct}%` }} aria-hidden />
          </div>
        </div>
      </div>

      {insight.sourceMemoryIds.length > 0 && (
        <div className="text-[11px] text-muted-foreground space-x-1 flex flex-wrap gap-1">
          {insight.sourceMemoryIds.slice(0, 6).map((id) => (
            <a key={id} href={`/memories/${id}`} className="hover:underline font-mono" title={id}>
              {id.slice(0, 8)}
            </a>
          ))}
          {insight.sourceMemoryIds.length > 6 && <span>+{insight.sourceMemoryIds.length - 6}</span>}
        </div>
      )}
    </Card>
  );
}
