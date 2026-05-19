import { useEffect, useId, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface InfoTooltipProps {
  /** The explanation shown when the user hovers / focuses the trigger. */
  content: string;
  /**
   * Visible label for the trigger. Defaults to a small `i` icon —
   * passing an explicit label is useful when the trigger sits next to
   * a label and you want the icon to anchor on the right.
   */
  label?: string;
  /** Accessible name for the trigger button. Falls back to `content`. */
  ariaLabel?: string;
  className?: string;
}

/**
 * Inline "what does this mean?" affordance for jargon-heavy labels
 * (FSRS retention, storage strength, trust score, FSRS-6 lapses…).
 * The dashboard is read-only and metrics-dense, so unexplained terms
 * become a discoverability cliff for new users.
 *
 * Implementation notes:
 *
 * - Pure-CSS popover would force us to choose between "always pin
 *   above" and "always pin below" which breaks at viewport edges.
 *   Doing it in JS gives us focus + click + Esc + outside-click
 *   handling for free.
 * - We do NOT lazy-mount the tooltip — content is plain text and
 *   keeping it in the DOM lets screen readers announce the
 *   description via `aria-describedby` immediately, which is the
 *   whole point of this affordance.
 * - The trigger is a `<button>` (not `<span>`) so keyboard users can
 *   reach it with Tab. We use `type="button"` to avoid accidental
 *   form submissions when the tooltip lives inside a form.
 */
export function InfoTooltip({ content, label, ariaLabel, className }: InfoTooltipProps) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onDocumentMouseDown(e: MouseEvent) {
      const target = e.target as Node | null;
      if (!target) return;
      if (triggerRef.current?.contains(target)) return;
      if (popoverRef.current?.contains(target)) return;
      setOpen(false);
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        setOpen(false);
        triggerRef.current?.focus();
      }
    }
    document.addEventListener('mousedown', onDocumentMouseDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('mousedown', onDocumentMouseDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  return (
    <span className={cn('relative inline-flex items-center', className)}>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen((v) => !v)}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        aria-describedby={id}
        aria-expanded={open}
        aria-label={ariaLabel ?? content}
        className="inline-flex items-center justify-center w-3.5 h-3.5 rounded-full text-[10px] font-medium text-muted-foreground bg-muted hover:bg-accent hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1 transition-colors leading-none"
      >
        <span aria-hidden="true">{label ?? 'i'}</span>
      </button>
      {/* The popover stays mounted but is hidden when collapsed, so
          aria-describedby never points to a missing node — screen readers
          still announce the description on focus. */}
      <div
        ref={popoverRef}
        id={id}
        role="tooltip"
        className={cn(
          'absolute bottom-full left-1/2 -translate-x-1/2 mb-2 z-50 w-64 max-w-[calc(100vw-2rem)]',
          'rounded-lg border border-border bg-card shadow-lg p-3 text-[11px] leading-relaxed text-card-foreground',
          'transition-opacity duration-100',
          open ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none',
        )}
      >
        {content}
      </div>
    </span>
  );
}
