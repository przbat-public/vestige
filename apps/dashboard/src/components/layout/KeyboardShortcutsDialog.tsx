import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';

interface KeyboardShortcutsDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * Cheat sheet of every keyboard shortcut the dashboard registers. Power
 * users discovered ⌘K and ⌘N by accident or by reading the source —
 * this dialog turns that tribal knowledge into a first-class affordance
 * triggered by `?` (or ⌘/ for keyboards where `?` is shift-bound).
 *
 * Groups: global (Layout), memories (MemoriesPage), in-modal.
 * Stays purely declarative — adding a shortcut means appending a row
 * here and registering the handler at the call site.
 */
interface ShortcutRow {
  /** i18n key for the action label. */
  labelKey: string;
  /**
   * Pre-rendered key tokens. Mac uses `⌘` glyph; we still spell out
   * `Ctrl` for users on linux/windows so the chip never lies about
   * which modifier is bound on their platform. The runtime detection
   * happens at module load — close enough for a cheat sheet.
   */
  keys: string[];
}

interface ShortcutGroup {
  titleKey: string;
  rows: ShortcutRow[];
}

const isMac = typeof navigator !== 'undefined' && /mac/i.test(navigator.platform);
const META = isMac ? '⌘' : 'Ctrl';

const GROUPS: ShortcutGroup[] = [
  {
    titleKey: 'shortcuts.group.global',
    rows: [
      { labelKey: 'shortcuts.commandPalette', keys: [META, 'K'] },
      { labelKey: 'shortcuts.addMemory', keys: [META, 'N'] },
      { labelKey: 'shortcuts.openHelp', keys: ['?'] },
      { labelKey: 'shortcuts.skipToContent', keys: ['Tab'] },
    ],
  },
  {
    titleKey: 'shortcuts.group.memories',
    rows: [
      { labelKey: 'shortcuts.selectAll', keys: [META, 'A'] },
      { labelKey: 'shortcuts.clearSelection', keys: ['Esc'] },
      { labelKey: 'shortcuts.cursorDown', keys: ['j'] },
      { labelKey: 'shortcuts.cursorUp', keys: ['k'] },
      { labelKey: 'shortcuts.cursorOpen', keys: ['Enter'] },
      { labelKey: 'shortcuts.cursorToggle', keys: ['x'] },
    ],
  },
  {
    titleKey: 'shortcuts.group.dialogs',
    rows: [
      { labelKey: 'shortcuts.close', keys: ['Esc'] },
      { labelKey: 'shortcuts.confirm', keys: ['Enter'] },
      { labelKey: 'shortcuts.navigateList', keys: ['↑', '↓'] },
    ],
  },
];

export function KeyboardShortcutsDialog({ open, onClose }: KeyboardShortcutsDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useRef<HTMLDialogElement>(null);

  // We mirror CommandPalette's strategy: `<dialog>` element with
  // `showModal()` so the browser handles backdrop + focus trap + ESC
  // for free. The component is uncontrolled w.r.t. visibility but
  // `open` is the external source of truth.
  useEffect(() => {
    if (open) {
      dialogRef.current?.showModal();
    } else {
      dialogRef.current?.close();
    }
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      onClose={onClose}
      className="fixed inset-0 z-50 m-0 p-0 w-full h-full max-w-none max-h-none bg-transparent backdrop:bg-black/40 dark:backdrop:bg-black/60"
      aria-labelledby="shortcuts-title"
    >
      <div className="flex items-start justify-center pt-[15vh] w-full h-full">
        <button
          type="button"
          className="fixed inset-0 cursor-default bg-transparent border-none"
          onClick={onClose}
          aria-label={t('shortcuts.closeAria')}
          tabIndex={-1}
        />
        <div className="relative glass-panel rounded-2xl w-full max-w-lg p-6 space-y-4 z-10">
          <header className="space-y-1">
            <h2 id="shortcuts-title" className="text-lg font-bold text-foreground">
              {t('shortcuts.title')}
            </h2>
            <p className="text-xs text-muted-foreground">{t('shortcuts.subtitle')}</p>
          </header>

          <div className="space-y-5">
            {GROUPS.map((group) => (
              <section key={group.titleKey} className="space-y-2">
                <h3 className="text-[11px] uppercase tracking-wider font-semibold text-muted-foreground">
                  {t(group.titleKey)}
                </h3>
                <ul className="space-y-1.5">
                  {group.rows.map((row) => (
                    <li key={row.labelKey} className="flex items-center justify-between gap-3 text-sm">
                      <span className="text-foreground">{t(row.labelKey)}</span>
                      <span className="flex items-center gap-1 shrink-0">
                        {row.keys.map((key, i) => (
                          // Composite key (label + key + position) — a Cmd+/
                          // shortcut can legitimately repeat the same key string
                          // (rare, but e.g. "K" appearing twice in a chord),
                          // and we still need stable React identity per row.
                          // biome-ignore lint/suspicious/noArrayIndexKey: index disambiguates duplicate-key chords
                          <span key={`${row.labelKey}-${key}-${i}`} className="flex items-center gap-1">
                            <kbd className="px-2 py-0.5 rounded-md bg-muted border border-border text-[11px] font-mono text-foreground min-w-[24px] text-center">
                              {key}
                            </kbd>
                            {i < row.keys.length - 1 && (
                              <span className="text-muted-foreground text-[10px]" aria-hidden="true">
                                +
                              </span>
                            )}
                          </span>
                        ))}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>

          <footer className="pt-3 border-t border-border text-[11px] text-muted-foreground">
            {t('shortcuts.footer')}
          </footer>
        </div>
      </div>
    </dialog>
  );
}
