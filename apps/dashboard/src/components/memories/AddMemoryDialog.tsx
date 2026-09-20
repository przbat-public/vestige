import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useEffect, useId } from 'react';
import { useForm } from 'react-hook-form';
import { useTranslation } from 'react-i18next';
import { z } from 'zod';
import { Alert } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { NativeSelect } from '@/components/ui/native-select';
import { Textarea } from '@/components/ui/textarea';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import type { SmartIngestFinding, SmartIngestResult } from '@/stores/api';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { toast } from '@/stores/toast';
import { SelfContainedFindings } from './SelfContainedFindings';

/**
 * Node types we expose in the dashboard form. Aligned with the canonical set
 * defined in `vestige_core::IngestInput::node_type` — keep in sync if new
 * types are added there. We intentionally omit `place` from the picker
 * because place memories generally come from preprocessing, not manual
 * entry.
 */
const NODE_TYPES = ['fact', 'concept', 'event', 'person', 'note', 'pattern', 'decision'] as const;

const schema = z.object({
  // 1MB hard ceiling matches the backend `tools::smart_ingest::execute`
  // `Content too large (max 1MB)` check — keeping the constraint client-side
  // means the user gets feedback before we round-trip to the server.
  content: z.string().min(1, 'addMemory.errors.contentRequired').max(1_000_000, 'addMemory.errors.contentTooLarge'),
  nodeType: z.enum(NODE_TYPES),
  tagsRaw: z.string().optional(),
  source: z.string().optional(),
  forceCreate: z.boolean().optional(),
});

type AddMemoryForm = z.infer<typeof schema>;

/**
 * What the engine actually did with the draft.
 *
 * The dialog used to branch on `decision === 'create' || decision ===
 * 'supersede'` and treat every other decision as a processed write. A refusal
 * arrives as HTTP 200 with `stored: false`, so it fell into the `else` branch,
 * the dialog closed over text that was never written, and the user's only clue
 * was a toast that read "smart-ingest decided to reject". Classifying once, in
 * one place, keeps the toast, the inline alert and the close/reset decision
 * from ever disagreeing again.
 */
type IngestOutcome =
  | { kind: 'refused'; findings: SmartIngestFinding[] }
  | { kind: 'flagged'; findings: SmartIngestFinding[] }
  | { kind: 'warned' }
  | { kind: 'stored' };

function classifyIngest(result: SmartIngestResult): IngestOutcome {
  if (result.stored === false || result.decision === 'reject') {
    // Refusals carry their findings at the top level; a stored response that
    // the gate flagged carries them under `self_contained`.
    return { kind: 'refused', findings: result.findings ?? result.self_contained?.findings ?? [] };
  }
  const findings = result.self_contained?.findings ?? [];
  if (result.self_contained?.requiresContext || findings.length > 0) {
    return { kind: 'flagged', findings };
  }
  if (result.compound_content_warning || result.near_duplicate_warning) {
    return { kind: 'warned' };
  }
  return { kind: 'stored' };
}

interface AddMemoryDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * Modal form to create a memory through the smart-ingest pipeline.
 *
 * Surfaces the engine's decision back to the user — when smart_ingest
 * decides to UPDATE/MERGE/SUPERSEDE rather than CREATE, we show that
 * outcome inline rather than silently pretending we created a new node.
 * This is the same affordance the MCP tool clients see, exposed for
 * humans.
 *
 * Accessibility:
 * - role="dialog" + aria-modal + aria-labelledby/described-by
 * - Escape closes
 * - Background click closes (a non-interactive div acts as the scrim,
 *   wired with a keyboard handler so SR users can still dismiss)
 * - Initial focus lands on the textarea (autoFocus)
 * - On close, focus returns to whatever triggered the dialog so the
 *   user lands back where they were (WAI-ARIA APG modal pattern).
 * - `useFocusTrap` keeps Tab inside the dialog. `aria-modal="true"` claims
 *   the rest of the page is inert, so the dialog has to make that true
 *   rather than rely on the user shift+Tab-ing back.
 */
export function AddMemoryDialog({ open, onClose }: AddMemoryDialogProps) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const titleId = useId();
  const descId = useId();
  // Owns focus restore too: the trap remembers the opener (⌘N, the FAB, an
  // empty-state CTA) and hands focus back to it on close.
  const dialogRef = useFocusTrap<HTMLDivElement>({ active: open });

  const {
    register,
    handleSubmit,
    reset,
    formState: { errors },
  } = useForm<AddMemoryForm>({
    resolver: zodResolver(schema),
    defaultValues: { content: '', nodeType: 'fact', tagsRaw: '', source: '', forceCreate: false },
  });

  const mutation = useMutation({
    mutationFn: api.memories.smartIngest,
    onSuccess: (result) => {
      // The backend WebSocket emit triggers cache invalidation already, but
      // we kick a manual invalidation as a belt-and-braces measure for the
      // case where the WS connection is degraded or in flight.
      qc.invalidateQueries({ queryKey: queryKeys.memoriesPrefix });
      qc.invalidateQueries({ queryKey: queryKeys.stats });

      const outcome = classifyIngest(result);

      // A refusal is not a save. Nothing was written, so the dialog stays open
      // with the draft intact — closing it here is exactly how the typed
      // content was thrown away while the toast claimed the write had been
      // processed. The inline Alert renders `reason`, `guidance` and `findings`
      // so the user can edit and retry without leaving the dialog.
      if (outcome.kind === 'refused') {
        toast(t('addMemory.toastRejected'), 'error');
        return;
      }

      // Written, and flagged: it exists and is searchable, but it leans on the
      // conversation it came from. Surfacing the findings is the whole point of
      // the gate, so this must not be toasted as a clean save either — keep the
      // dialog open so the findings stay next to the text they describe.
      if (outcome.kind === 'flagged') {
        toast(t('addMemory.toastFlagged'), 'info');
        return;
      }

      // The backend may flag the write with a `compound_content_warning`
      // or `near_duplicate_warning` that is actionable — the user should
      // either split the memory into atomics or re-submit with
      // `forceCreate` to acknowledge a near-duplicate. Auto-closing the
      // dialog hides that advice behind a toast and forces the user to
      // navigate to the new memory just to read it. Keep the dialog open
      // so the inline Alert (rendered below) can deliver the message
      // where the user can still act on it.
      if (outcome.kind === 'warned') {
        toast(t('addMemory.toastWithWarning'), 'info');
        return;
      }

      // Translate the engine's decision into a human message rather than
      // dumping the raw `decision` string. The decision shapes the toast:
      // create/supersede → success ("Created"); reinforce/update/merge →
      // info ("Merged with existing memory"). Both close the dialog —
      // either way the user's input was successfully written.
      const decision = result.decision;
      if (decision === 'create' || decision === 'supersede') {
        toast(t('addMemory.toastCreated'), 'success');
      } else {
        toast(t('addMemory.toastMerged', { decision }), 'info');
      }
      reset();
      onClose();
    },
    onError: (err: Error) => {
      // Smart_ingest can return a 400 with a useful detail message
      // (compound content too long, validation error). Surface it verbatim
      // rather than the generic "common.error".
      toast(err.message || t('common.error'), 'error');
    },
  });

  // Reset form whenever the dialog closes so the next open starts fresh.
  // We don't want stale draft content reappearing — that violates the
  // principle of least surprise for a "create new" affordance.
  useEffect(() => {
    if (!open) reset();
  }, [open, reset]);

  // Escape closes. We bind on `window` rather than the dialog element so
  // it works even when focus is somewhere weird (toast, scrim).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  const onSubmit = (data: AddMemoryForm) => {
    const tags =
      data.tagsRaw
        ?.split(',')
        .map((t) => t.trim())
        .filter(Boolean) ?? [];
    mutation.mutate({
      content: data.content,
      tags,
      nodeType: data.nodeType,
      source: data.source || undefined,
      forceCreate: data.forceCreate,
    });
  };

  return (
    // The scrim is a non-interactive div (it can be clicked but it's not
    // a button) — using `<button>` here flagged Lighthouse as wrong
    // semantics (a button that wraps a dialog isn't meaningful to AT)
    // and broke `aria-modal` semantics on some screen readers, which
    // expect the dialog and scrim to live as siblings. Keyboard users
    // dismiss with ESC; mouse/touch users still get click-outside.
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      {/* Scrim — role="presentation" makes it transparent to AT, ESC is
          handled by the window listener above. */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: scrim is a click target, not an interactive control */}
      <div role="presentation" className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onClose} />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descId}
        className="relative w-full max-w-lg max-h-[90vh] overflow-y-auto bg-card border border-border rounded-xl shadow-2xl"
      >
        <form onSubmit={handleSubmit(onSubmit)} className="p-6 space-y-4">
          <div>
            <h2 id={titleId} className="text-lg font-bold text-foreground">
              {t('addMemory.title')}
            </h2>
            <p id={descId} className="text-xs text-muted-foreground mt-1">
              {t('addMemory.description')}
            </p>
          </div>

          <div>
            <label htmlFor="add-memory-content" className="text-xs text-muted-foreground block mb-1">
              {t('addMemory.contentLabel')}
            </label>
            <Textarea
              id="add-memory-content"
              rows={5}
              autoFocus
              placeholder={t('addMemory.contentPlaceholder')}
              aria-invalid={!!errors.content}
              {...register('content')}
            />
            {errors.content?.message && <p className="text-xs text-red-500 mt-1">{t(errors.content.message)}</p>}
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label htmlFor="add-memory-type" className="text-xs text-muted-foreground block mb-1">
                {t('addMemory.nodeTypeLabel')}
              </label>
              <NativeSelect id="add-memory-type" {...register('nodeType')} className="w-full">
                {NODE_TYPES.map((nt) => (
                  <option key={nt} value={nt}>
                    {t(`nodeTypes.${nt}`, { defaultValue: nt })}
                  </option>
                ))}
              </NativeSelect>
            </div>
            <div>
              <label htmlFor="add-memory-source" className="text-xs text-muted-foreground block mb-1">
                {t('addMemory.sourceLabel')}
              </label>
              <Input id="add-memory-source" placeholder={t('addMemory.sourcePlaceholder')} {...register('source')} />
            </div>
          </div>

          <div>
            <label htmlFor="add-memory-tags" className="text-xs text-muted-foreground block mb-1">
              {t('addMemory.tagsLabel')}
            </label>
            <Input id="add-memory-tags" placeholder={t('addMemory.tagsPlaceholder')} {...register('tagsRaw')} />
            <p className="text-[10px] text-muted-foreground mt-1">{t('addMemory.tagsHint')}</p>
          </div>

          <label className="flex items-start gap-2 text-xs text-muted-foreground cursor-pointer">
            <input
              type="checkbox"
              {...register('forceCreate')}
              className="mt-0.5 rounded border-border bg-background"
            />
            <span>
              <span className="font-medium text-foreground">{t('addMemory.forceCreateLabel')}</span>
              <br />
              <span>{t('addMemory.forceCreateHint')}</span>
            </span>
          </label>

          {mutation.isError && (
            <Alert variant="destructive">
              <p className="font-medium mb-1">{t('common.error')}</p>
              <p>{mutation.error instanceof Error ? mutation.error.message : t('addMemory.errors.unknown')}</p>
            </Alert>
          )}

          {mutation.data && <IngestResultAlert result={mutation.data} />}

          <div className="flex gap-2 justify-end pt-2 border-t border-border">
            <Button type="button" variant="ghost" onClick={onClose} disabled={mutation.isPending}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" variant="default" disabled={mutation.isPending}>
              {mutation.isPending ? t('common.loading') : t('addMemory.submit')}
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}

/** The findings a reader can act on — refusals and flagged writes only. */
function actionableFindings(outcome: IngestOutcome): SmartIngestFinding[] {
  return outcome.kind === 'refused' || outcome.kind === 'flagged' ? outcome.findings : [];
}

/**
 * The inline result of the last submit, shaped by what the engine actually did.
 *
 * `destructive` for a refusal (nothing was written), `warning` for a write the
 * gate flagged or for an actionable compound/near-duplicate warning, `success`
 * only for a clean create. The old version coloured the alert by
 * `decision === 'create'` alone, so a refusal — whose `decision` is `"reject"`
 * — rendered as a neutral box that the dialog closed a moment later.
 */
function IngestResultAlert({ result }: { result: SmartIngestResult }) {
  const { t } = useTranslation();
  const outcome = classifyIngest(result);
  const findings = actionableFindings(outcome);

  const variant =
    outcome.kind === 'stored' && result.decision === 'create' ? 'success' : OUTCOME_ALERT_VARIANT[outcome.kind];
  const title =
    outcome.kind === 'refused' || outcome.kind === 'flagged'
      ? t(`addMemory.${outcome.kind === 'refused' ? 'rejectTitle' : 'selfContainedTitle'}`)
      : t(`addMemory.decision.${result.decision}`, { defaultValue: result.decision });
  // A refusal's own `reason` is the claim that matters; `explanation` describes
  // a write decision and a refusal never has one. Preferring the decision text
  // there would print the wrong story next to "not saved".
  const body = outcome.kind === 'refused' ? (result.reason ?? result.explanation) : result.explanation || result.reason;

  return (
    <Alert variant={variant}>
      <p className="font-medium mb-1">{title}</p>
      <p>{body}</p>

      {result.guidance && (
        <p className="mt-2">
          <span className="font-medium">{t('addMemory.guidanceLabel')}: </span>
          {result.guidance}
        </p>
      )}

      {findings.length > 0 && <p className="mt-2 font-medium">{t('selfContained.findingsTitle')}</p>}
      <SelfContainedFindings findings={findings} />

      {outcome.kind === 'refused' && <p className="mt-2 font-medium">{t('addMemory.rejectHint')}</p>}
      {outcome.kind === 'flagged' && <p className="mt-2">{t('addMemory.selfContainedHint')}</p>}

      {result.compound_content_warning && <AdvisoryNotice kind="compound" text={result.compound_content_warning} />}
      {result.near_duplicate_warning && <AdvisoryNotice kind="nearDuplicate" text={result.near_duplicate_warning} />}
    </Alert>
  );
}

/** Alert colour per outcome; `stored` is refined by the decision in the caller. */
const OUTCOME_ALERT_VARIANT = {
  refused: 'destructive',
  flagged: 'warning',
  warned: 'warning',
  stored: 'default',
} as const;

/** One of smart-ingest's two advisory warnings, badge plus message. */
function AdvisoryNotice({ kind, text }: { kind: 'compound' | 'nearDuplicate'; text: string }) {
  const { t } = useTranslation();
  const label = kind === 'compound' ? t('addMemory.compoundWarning') : t('addMemory.nearDuplicateWarning');
  return (
    <p className="mt-2">
      <Badge variant="warning" className="mr-1">
        {label}
      </Badge>
      {text}
    </p>
  );
}
