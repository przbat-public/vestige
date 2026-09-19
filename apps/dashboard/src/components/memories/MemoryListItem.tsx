import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Checkbox } from '@/components/ui/checkbox';
import { ProgressBar } from '@/components/ui/progress-bar';
import { usePinned } from '@/stores/pinned';
import type { Memory } from '@/types';
import { EPISTEMIC_STATUS_COLORS, NODE_TYPE_COLORS, retentionColor } from '@/types';

interface MemoryListItemProps {
  memory: Memory;
  /** Highlighted as the "active" row in the detail panel. */
  isActive: boolean;
  /** Member of the bulk-selection set. */
  isSelected: boolean;
  /**
   * The list as a whole has at least one selected memory. Drives "selection
   * mode": checkbox is always visible (vs. on hover) so the user can keep
   * clicking through rows fluidly.
   */
  selectionMode: boolean;
  /** True when the j/k cursor is on this row. Renders a subtle focus ring. */
  isFocused?: boolean;
  /**
   * Pinned state. Passed in by the list so a row does not have to subscribe to
   * the pinned store itself: with 100 rendered rows, every pin/unpin used to
   * fire 100 `localStorage` reads (one per row's `usePinned`) and re-render the
   * whole list. Omit it only for standalone rows.
   */
  isPinned?: boolean;
  onActivate: (m: Memory) => void;
  onToggleSelect: (id: string, opts: { shift?: boolean; ctrl?: boolean }) => void;
}

/**
 * One row in the memories list.
 *
 * Click vs shift-click vs ctrl/cmd-click semantics (Linear / Finder):
 * - plain click → set this row as the active detail target (does not affect selection).
 * - cmd/ctrl + click → toggle this row in the bulk-selection set.
 * - shift + click → range-select from the last anchor to this row.
 * - checkbox click → toggle this row in the bulk-selection set.
 *
 * The two affordances coexist because users often want to scan/preview rows
 * (plain click) without losing their selection state. This matches Linear's
 * "click peeks, ctrl-click selects" pattern.
 */
function MemoryListItemRow({
  memory,
  isActive,
  isSelected,
  selectionMode,
  isFocused = false,
  isPinned: pinnedProp,
  onActivate,
  onToggleSelect,
}: MemoryListItemProps) {
  const { t } = useTranslation();
  // Fallback for standalone usage; the list passes the flag down so the
  // subscription stays at one per page instead of one per row.
  const pinned = usePinned();
  const isPinned = pinnedProp ?? pinned.has(memory.id);

  const handleClick = (e: React.MouseEvent<HTMLButtonElement>) => {
    if (e.shiftKey) {
      e.preventDefault();
      onToggleSelect(memory.id, { shift: true });
      return;
    }
    if (e.metaKey || e.ctrlKey) {
      e.preventDefault();
      onToggleSelect(memory.id, { ctrl: true });
      return;
    }
    onActivate(memory);
  };

  const handleCheckboxChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    // Forward the underlying mouse modifiers so a shift-click on the checkbox
    // also triggers a range-select (matches Notion/Asana behavior).
    const native = e.nativeEvent as MouseEvent;
    onToggleSelect(memory.id, { shift: native.shiftKey });
  };

  return (
    <div
      data-memory-row={memory.id}
      className={`group relative flex items-stretch rounded-lg transition min-w-0 ${
        isSelected
          ? 'bg-primary/5 border border-primary/40'
          : isActive
            ? 'bg-primary/10 border-l-2 border-primary'
            : 'hover:bg-accent border border-transparent'
      } ${isFocused ? 'ring-2 ring-ring ring-offset-1 ring-offset-background' : ''}`}
    >
      {/* biome-ignore lint/a11y/noLabelWithoutControl: <Checkbox/> renders a real <input type="checkbox"> that the label implicitly associates with */}
      <label
        className={`flex items-center justify-center pl-2 pr-1 cursor-pointer ${
          selectionMode || isSelected ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100'
        } transition-opacity`}
      >
        <span className="sr-only">{t('bulk.toggleAriaLabel', { content: memory.content.slice(0, 40) })}</span>
        <Checkbox checked={isSelected} onChange={handleCheckboxChange} onClick={(e) => e.stopPropagation()} />
      </label>
      <button
        type="button"
        onClick={handleClick}
        className="flex-1 text-left px-2 py-2.5 min-w-0 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg"
        aria-current={isActive ? 'true' : undefined}
      >
        <div className="flex items-center gap-2">
          <span
            className="w-2 h-2 rounded-full flex-shrink-0"
            style={{ backgroundColor: NODE_TYPE_COLORS[memory.nodeType] || '#8B95A5' }}
            aria-hidden="true"
          />
          {isPinned && (
            <span
              role="img"
              aria-label={t('memories.pinnedAria')}
              title={t('memories.pinnedAria')}
              className="text-amber-500 text-[11px] leading-none flex-shrink-0"
            >
              ★
            </span>
          )}
          <span className="text-xs text-foreground truncate">{memory.content.slice(0, 80)}</span>
        </div>
        <div className="flex items-center gap-2 mt-1 text-xs">
          <span className="text-muted-foreground">
            {t(`nodeTypes.${memory.nodeType}`, { defaultValue: memory.nodeType })}
          </span>
          {memory.epistemicStatus && (
            <span className="text-xs" style={{ color: EPISTEMIC_STATUS_COLORS[memory.epistemicStatus] || '#8B95A5' }}>
              {t(`epistemic.${memory.epistemicStatus}`)}
            </span>
          )}
          <ProgressBar
            value={memory.retentionStrength * 100}
            label={t('memories.retention')}
            color={retentionColor(memory.retentionStrength)}
            className="flex-1 max-w-[80px]"
          />
        </div>
      </button>
    </div>
  );
}

/**
 * Memoised: `MemoriesPage` renders up to `PAGE_SIZE` (100) rows in one list,
 * and the search box is controlled at the page level, so every keystroke used
 * to re-render all of them. Props are primitives plus two `useCallback`-stable
 * handlers, so the shallow comparison is effective.
 */
export const MemoryListItem = memo(MemoryListItemRow);
