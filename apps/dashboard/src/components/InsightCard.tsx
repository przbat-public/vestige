import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { InfoTooltip } from '@/components/ui/info-tooltip';
import { useMemoryMutations } from '@/hooks/useMemoryMutations';
import { useDialogStore } from '@/stores/dialogs';
import type { Insight } from '@/types';

/**
/**
 * Render one synthesised Insight Tier card.
 *
 * The dream cycle and `reflect` tool emit tentative observations
 * ("3 of your last 5 OAuth bugs were token-cache invalidation
 * issues"). Each one becomes a `KnowledgeNode` of type `"insight"`
 * with structured metadata under `extra_json.insight`. This card
 * surfaces them and lets the reviewer mark them validated or dismiss
 * them inline — both buttons route through `useMemoryMutations`
 * (which calls the same `/memories/:id/promote` and `/demote`
 * endpoints the MCP `memory(action="promote"|"demote")` actions hit).
 *
 * Promoting an Insight node has a side effect at the backend layer:
 * it flips `extra_json.insight.validatedByAgent` to `true` and stamps
 * `validatedAt`. The "Validate" button in this card is just the UX
 * shortcut for that flow — pre-v3.4.1 the tutorial promised this
 * affordance but no button existed.
 *
 * Visual cues:
 *   * Unvalidated → yellow "Unvalidated" badge, action row visible.
 *   * Validated   → green "Validated" badge, subtle ring around the card,
 *                   Validate button replaced by a disabled checkmark.
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
  const navigate = useNavigate();
  const createdAt = new Date(insight.createdAt);
  const validatedAt = insight.validatedAt ? new Date(insight.validatedAt) : null;
  const locale = i18n.language;

  const confidencePct = Math.round(insight.confidence * 100);
  const noveltyPct = Math.round(insight.novelty * 100);

  const { promote, demote } = useMemoryMutations();
  const validatePending = promote.isPending && promote.variables === insight.id;
  const demotePending = demote.isPending && demote.variables === insight.id;

  // Same pattern as HubCard / CommandPalette: stash the id and navigate to
  // /memories, where MemoriesPage drains the slot and opens the drawer.
  // The dashboard has no /memories/:id route — a raw anchor reloaded into
  // a 404 and dropped the user out of context.
  const openMemory = (id: string) => {
    useDialogStore.getState().requestSelectMemory(id);
    navigate('/memories');
  };

  return (
    <Card className={insight.validated ? 'space-y-3 ring-1 ring-emerald-500/40' : 'space-y-3'}>
      <div className="space-y-1">
        <div className="flex items-start justify-between gap-3 flex-wrap">
          {/* The insight is itself a KnowledgeNode — clicking the
              content routes back to the host memory so the user can
              inspect tags, retention, edit history, and the graph
              neighborhood. Source-id chips below open *their* memories;
              this button opens the insight's own memory. */}
          <button
            type="button"
            onClick={() => openMemory(insight.id)}
            className="text-sm text-foreground break-words flex-1 min-w-0 text-left hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-sm transition-colors"
          >
            {insight.content}
          </button>
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
          })}
          {validatedAt && (
            <>
              {' · '}
              {t('insights.validatedOn', {
                date: validatedAt.toLocaleDateString(locale),
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
              <InfoTooltip content={t('insights.confidenceTooltip')} />
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
              <InfoTooltip content={t('insights.noveltyTooltip')} />
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
            <button
              key={id}
              type="button"
              onClick={() => openMemory(id)}
              className="hover:underline font-mono cursor-pointer"
              title={id}
            >
              {id.slice(0, 8)}
            </button>
          ))}
          {insight.sourceMemoryIds.length > 6 && <span>+{insight.sourceMemoryIds.length - 6}</span>}
        </div>
      )}

      <div className="flex items-center gap-2 pt-1 border-t border-border/40">
        {insight.validated ? (
          <span className="inline-flex items-center gap-1 text-xs text-emerald-600 dark:text-emerald-400">
            <span aria-hidden>✓</span>
            {t('insights.actions.alreadyValidated', 'Validated')}
          </span>
        ) : (
          <Button
            type="button"
            variant="success"
            size="sm"
            disabled={validatePending}
            onClick={() => promote.mutate(insight.id)}
            title={t('insights.actions.validateTooltip', 'Mark this insight as confirmed and boost its ranking.')}
            aria-label={t('insights.actions.validateTooltip', 'Mark this insight as confirmed and boost its ranking.')}
          >
            {validatePending
              ? t('insights.actions.validating', 'Validating…')
              : t('insights.actions.validate', 'Validate')}
          </Button>
        )}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={demotePending}
          onClick={() => demote.mutate(insight.id)}
          title={t(
            'insights.actions.demoteTooltip',
            'Lower the ranking so search surfaces better-supported memories instead.',
          )}
          aria-label={t(
            'insights.actions.demoteTooltip',
            'Lower the ranking so search surfaces better-supported memories instead.',
          )}
        >
          {demotePending ? t('insights.actions.demoting', 'Dismissing…') : t('insights.actions.demote', 'Dismiss')}
        </Button>
      </div>
    </Card>
  );
}
