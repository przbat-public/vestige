import { useEffect, useId, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import { useConfirmResolve, useConfirmState } from '@/stores/confirm';

/**
 * Singleton confirm-dialog renderer. Mount once near the root (`Layout`)
 * — the actual `confirm()` calls live in `stores/confirm.ts` and resolve
 * a promise based on the user's choice.
 *
 * Accessibility:
 *  - `role="alertdialog"` per ARIA Authoring Practices (more weight than
 *    `dialog` for destructive intent; screen readers announce the body
 *    text and the focus moves to the confirm button by default).
 *  - `aria-modal="true"` + scrim catch + Escape close → standard modal
 *    interaction model.
 *  - Initial focus lands on the *cancel* button when the action is
 *    destructive, on the *confirm* button otherwise — same principle as
 *    macOS sheets ("don't make destruction the default").
 *  - `useFocusTrap` cycles Tab inside the dialog and restores focus to the
 *    opener on close. `aria-modal="true"` claims the rest of the document is
 *    inert; without the trap that claim was false and Tab reached the sidebar
 *    behind the scrim.
 */
export function ConfirmDialogHost() {
  const { t } = useTranslation();
  const pending = useConfirmState();
  const resolve = useConfirmResolve();
  const titleId = useId();
  const descId = useId();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  // Stable resolver for the keydown listener so the effect doesn't
  // re-bind on every render of the parent.
  const resolveRef = useRef(resolve);
  resolveRef.current = resolve;

  const destructive = pending?.destructive ?? false;
  const safeTarget = destructive ? cancelRef : confirmRef;
  const trapRef = useFocusTrap<HTMLDivElement>({ active: !!pending, initialFocus: safeTarget });

  // Escape always cancels (closing the dialog without the action).
  useEffect(() => {
    if (!pending) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        resolveRef.current(false);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [pending]);

  if (!pending) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      {/* Click-outside scrim. Not in the accessibility tree:
          - `role="presentation"` hides it from assistive tech (keyboard users get ESC + the Cancel button).
          - It exists only as a mouse-convenience surface; making it a button would double-label "Cancel" in the a11y tree. */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: scrim is a mouse-only click target; keyboard users have ESC + an explicit Cancel button */}
      <div
        role="presentation"
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
        onClick={() => resolve(false)}
      />
      <div
        ref={trapRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descId}
        tabIndex={-1}
        className="relative w-full max-w-md bg-card border border-border rounded-xl shadow-2xl p-6"
      >
        <h2 id={titleId} className="text-base font-semibold text-foreground mb-2">
          {pending.title ?? t('confirm.defaultTitle')}
        </h2>
        <p id={descId} className="text-sm text-muted-foreground whitespace-pre-line">
          {pending.message}
        </p>
        <div className="flex gap-2 justify-end mt-5">
          <Button ref={cancelRef} type="button" variant="ghost" onClick={() => resolve(false)}>
            {pending.cancelLabel ?? t('common.cancel')}
          </Button>
          <Button
            ref={confirmRef}
            type="button"
            variant={pending.destructive ? 'danger' : 'default'}
            onClick={() => resolve(true)}
          >
            {pending.confirmLabel ?? t('common.confirm')}
          </Button>
        </div>
      </div>
    </div>
  );
}
