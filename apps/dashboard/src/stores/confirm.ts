import { create } from 'zustand';

/**
 * Promise-based confirm dialog — drop-in replacement for `window.confirm`
 * that renders as an accessible modal (focus trap, ESC, Enter, `role="alertdialog"`).
 *
 * Why not just `window.confirm()`? Three reasons:
 *
 *  1. Browser confirms can't be styled, can't be themed, and behave
 *     inconsistently across browsers/OSes.
 *  2. They block the whole tab (synchronous), so React state work in
 *     flight gets paused — sometimes causing flicker on close.
 *  3. They make e2e/integration tests harder: Playwright / Vitest need
 *     special hooks to handle native dialogs.
 *
 * Usage:
 *   const ok = await confirm({
 *     message: t('bulk.deleteConfirm', { count }),
 *     destructive: true,
 *   });
 *   if (!ok) return;
 */
export interface ConfirmRequest {
  /** Body text. Pre-translated by the caller. */
  message: string;
  /** Optional heading. Defaults to a generic "Confirm" string. */
  title?: string;
  /** Custom label for the confirm button. Defaults to `common.confirm`. */
  confirmLabel?: string;
  /** Custom label for the cancel button. Defaults to `common.cancel`. */
  cancelLabel?: string;
  /** Highlights the confirm button as destructive (red) — for delete flows. */
  destructive?: boolean;
}

interface PendingConfirm extends ConfirmRequest {
  resolve: (value: boolean) => void;
}

interface ConfirmStore {
  pending: PendingConfirm | null;
  request: (req: ConfirmRequest) => Promise<boolean>;
  resolve: (value: boolean) => void;
}

const useConfirmStore = create<ConfirmStore>((set, get) => ({
  pending: null,
  request: (req) =>
    new Promise<boolean>((resolve) => {
      // If a previous confirm was somehow still open, resolve it as
      // cancelled — the caller of the new confirm becomes the owner.
      // This shouldn't happen in normal flow (we only request from
      // user actions) but protects against double-clicks.
      const prev = get().pending;
      if (prev) prev.resolve(false);
      set({ pending: { ...req, resolve } });
    }),
  resolve: (value) => {
    const pending = get().pending;
    if (!pending) return;
    pending.resolve(value);
    set({ pending: null });
  },
}));

/**
 * Public helper. Use exactly like `window.confirm` — returns a promise
 * that resolves to `true` when the user confirms, `false` when they
 * cancel or close the modal.
 */
export function confirm(req: ConfirmRequest): Promise<boolean> {
  return useConfirmStore.getState().request(req);
}

export function useConfirmState() {
  return useConfirmStore((s) => s.pending);
}

export function useConfirmResolve() {
  return useConfirmStore((s) => s.resolve);
}
