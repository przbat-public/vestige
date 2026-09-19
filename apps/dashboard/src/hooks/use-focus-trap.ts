import { type RefObject, useEffect, useRef } from 'react';

/**
 * Focus containment for modal dialogs that are not native `<dialog>` elements.
 *
 * `aria-modal="true"` tells assistive technology the rest of the document is
 * inert. For a `<dialog>` opened with `showModal()` the browser makes that
 * true; for a `<div role="dialog">` it is a promise the component has to keep
 * itself, otherwise Tab walks straight out into the sidebar behind the scrim
 * (WCAG 2.4.3, ARIA APG dialog pattern).
 *
 * Handles the three parts of that promise:
 *   1. Tab / Shift+Tab cycle within the dialog,
 *   2. focus is moved to `initialFocus` on open,
 *   3. focus returns to the element that opened it on close.
 *
 * Escape is deliberately *not* handled here — each dialog owns its own
 * dismissal semantics (confirm → resolve(false), help → close, …).
 */

const FOCUSABLE = [
  'a[href]',
  'area[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  'iframe',
  'audio[controls]',
  'video[controls]',
  '[contenteditable]:not([contenteditable="false"])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

function isVisible(element: HTMLElement): boolean {
  if (element.hasAttribute('hidden')) return false;
  if (element.getAttribute('aria-hidden') === 'true') return false;
  return element.offsetParent !== null || element.getClientRects().length > 0;
}

/** Focusable descendants, in document order. */
export function getFocusable(container: HTMLElement | null): HTMLElement[] {
  if (!container) return [];
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (element) => element.tabIndex !== -1 && isVisible(element),
  );
}

/** Last focusable descendant — the wrap target for Shift+Tab. */
export function getLastFocusable(container: HTMLElement | null): HTMLElement | null {
  const focusable = getFocusable(container);
  return focusable[focusable.length - 1] ?? null;
}

interface Options {
  /** Whether the dialog is currently mounted/open. */
  active: boolean;
  /** Element to focus on open; defaults to the first focusable descendant. */
  initialFocus?: RefObject<HTMLElement | null>;
  /** Turn the focus trap off without unmounting (rarely needed). */
  enabled?: boolean;
}

/** Wrap Tab / Shift+Tab at the edges of `container`. */
function cycleFocus(event: KeyboardEvent, container: HTMLElement): void {
  const focusable = getFocusable(container);
  if (focusable.length === 0) {
    // Nothing focusable inside: keep focus on the container itself rather
    // than letting Tab escape to the page behind.
    event.preventDefault();
    container.focus({ preventScroll: true });
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const current = document.activeElement as HTMLElement | null;
  const inside = current ? container.contains(current) : false;

  if (!inside) {
    event.preventDefault();
    (event.shiftKey ? last : first).focus({ preventScroll: true });
    return;
  }
  if (event.shiftKey && current === first) {
    event.preventDefault();
    last.focus({ preventScroll: true });
  } else if (!event.shiftKey && current === last) {
    event.preventDefault();
    first.focus({ preventScroll: true });
  }
}

export function useFocusTrap<T extends HTMLElement>({ active, initialFocus, enabled = true }: Options) {
  const containerRef = useRef<T>(null);
  // Captured in a layout-independent way: read at activation time so the
  // opener is whatever had focus when the dialog opened.
  const restoreRef = useRef<HTMLElement | null>(null);
  // Read through a ref so a caller passing a fresh ref object each render
  // does not re-activate the trap (and re-focus the dialog mid-interaction).
  const initialFocusRef = useRef(initialFocus);
  initialFocusRef.current = initialFocus;

  useEffect(() => {
    if (!active || !enabled) return;

    restoreRef.current = (document.activeElement as HTMLElement | null) ?? null;

    // Move focus in. `preventScroll` keeps the page from jumping when the
    // dialog is taller than the viewport.
    const preferred = initialFocusRef.current?.current;
    const target = preferred ?? getFocusable(containerRef.current)[0] ?? containerRef.current;
    target?.focus({ preventScroll: true });

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const container = containerRef.current;
      if (container) cycleFocus(event, container);
    };

    document.addEventListener('keydown', onKeyDown, true);
    return () => {
      document.removeEventListener('keydown', onKeyDown, true);
      // Hand focus back to the opener — but only if it is still in the
      // document (the row it belonged to may have been deleted).
      const opener = restoreRef.current;
      if (opener?.isConnected) {
        opener.focus({ preventScroll: true });
      }
    };
  }, [active, enabled]);

  return containerRef;
}
