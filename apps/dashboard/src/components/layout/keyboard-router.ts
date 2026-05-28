/**
 * Helpers shared by the global keyboard router (Layout) — extracted so
 * the precedence logic can be unit-tested in isolation, without
 * spinning up the full Router/Query/Theme provider stack.
 *
 * Each predicate is documented at the rule level (what it stops, why),
 * because each carve-out has cost a real bug at some point:
 *
 * - `isTypingInEditable` — bare `?` / `n` chords would otherwise
 *   intercept every literal `?` / `n` keystroke users typed into
 *   search boxes and forms.
 * - `isInsideApplicationWidget` — the `role="application"` opt-out
 *   yields keyboard control to screen-reader virtual cursors. We
 *   piggy-back on the same boundary to let widgets like Graph3D own
 *   their own `?` shortcut without the global router firing in
 *   parallel and opening two help dialogs.
 */

/**
 * Whether the active target is an editable surface that should
 * suppress single-character global shortcuts (bare `?`, `n`, etc).
 * Composite chords (`⌘K`, `⌘N`, …) bypass this guard because the
 * meta key disambiguates them from regular typing.
 */
export function isTypingInEditable(target: EventTarget | null): boolean {
  if (!target) return false;
  if (typeof Element !== 'undefined' && target instanceof Element) {
    if (target instanceof HTMLInputElement) return true;
    if (target instanceof HTMLTextAreaElement) return true;
    if (target instanceof HTMLElement && target.isContentEditable) return true;
  }
  return false;
}

/**
 * Whether the focused element sits inside a `role="application"`
 * subtree (or is one itself). The ARIA contract for that role is
 * "this widget intercepts keys", so the global router yields to
 * whatever shortcut handler the widget has wired up. Used to prevent
 * the global `?` from racing the per-page help overlay on Graph3D.
 */
export function isInsideApplicationWidget(target: EventTarget | null): boolean {
  if (typeof Element === 'undefined' || !(target instanceof Element)) return false;
  return target.closest('[role="application"]') !== null;
}
