import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';

interface BulkActionBarProps {
  /** How many memories are currently selected. The bar hides when 0. */
  count: number;
  /** Whether any of the per-action mutations is currently running (disables buttons + shows loading hint). */
  busy: boolean;
  onPromote: () => void;
  onDemote: () => void;
  onDelete: () => void;
  onClear: () => void;
}

/**
 * Sticky bottom toolbar that appears when the user has selected memories.
 *
 * Linear / Notion convention: do not show actions until the user signals
 * intent (selects something). Once selected, the toolbar reveals available
 * verbs alongside the count and a clear button.
 *
 * Interaction notes:
 * - Esc clears the selection (handled by the parent so it can also bail
 *   out of edit-in-progress flows; we forward the click → onClear).
 * - Delete is destructive — visually flagged via the `danger` variant. The
 *   parent confirms before actually firing.
 * - aria-live="polite" so screen readers announce "5 selected" when the
 *   count changes without stealing focus away from the active row.
 */
export function BulkActionBar({ count, busy, onPromote, onDemote, onDelete, onClear }: BulkActionBarProps) {
  const { t } = useTranslation();
  if (count === 0) return null;

  return (
    <section
      aria-label={t('bulk.toolbarAriaLabel')}
      className="absolute left-1/2 -translate-x-1/2 bottom-4 z-20 flex flex-wrap items-center gap-2 bg-card/95 backdrop-blur-sm border border-border rounded-xl shadow-xl px-3 py-2"
    >
      <span aria-live="polite" className="text-xs font-medium text-foreground tabular-nums">
        {t('bulk.selectedCount', { count })}
      </span>
      <span aria-hidden="true" className="text-muted-foreground">
        ·
      </span>
      <Button type="button" variant="success" size="sm" onClick={onPromote} disabled={busy} className="text-xs">
        ↑ {t('memories.promote')}
      </Button>
      <Button type="button" variant="danger" size="sm" onClick={onDemote} disabled={busy} className="text-xs">
        ↓ {t('memories.demote')}
      </Button>
      <Button type="button" variant="danger" size="sm" onClick={onDelete} disabled={busy} className="text-xs">
        ✕ {t('common.delete')}
      </Button>
      <span aria-hidden="true" className="text-muted-foreground">
        ·
      </span>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={onClear}
        disabled={busy}
        className="text-xs"
        aria-label={t('bulk.clearAriaLabel')}
      >
        {t('bulk.clear')}
      </Button>
      {busy && <span className="text-[10px] text-muted-foreground italic">{t('common.loading')}</span>}
    </section>
  );
}
