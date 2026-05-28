import { useEffect, useId, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';

interface Props {
  open: boolean;
  onClose: () => void;
}

interface Shortcut {
  /**
   * Pre-formatted key glyph(s) — kept literal so we don't translate `Esc`
   * to `Esc` in every language. The displayed key is the same on every
   * keyboard; only the *description* needs localisation.
   */
  keys: string[];
  /** i18n key under `graph.help.keys.*`. */
  descKey: string;
}

interface Section {
  /** i18n key under `graph.help.sections.*`. */
  titleKey: string;
  shortcuts: Shortcut[];
}

// The matrix is laid out top-down by frequency of use — the user opens
// this overlay because they're stuck, so the answer to "how do I move
// around" should be the first thing they see.
const SECTIONS: Section[] = [
  {
    titleKey: 'navigate',
    shortcuts: [
      { keys: ['←', '→', '↑', '↓'], descKey: 'arrows' },
      { keys: ['Enter'], descKey: 'enter' },
      { keys: ['Esc'], descKey: 'escape' },
      { keys: ['Home'], descKey: 'home' },
      { keys: ['End'], descKey: 'end' },
    ],
  },
  {
    titleKey: 'frame',
    shortcuts: [
      { keys: ['F'], descKey: 'frameNode' },
      { keys: ['A'], descKey: 'frameAll' },
      { keys: ['R'], descKey: 'reset' },
    ],
  },
  {
    titleKey: 'history',
    shortcuts: [
      { keys: ['Alt', '←'], descKey: 'back' },
      { keys: ['Alt', '→'], descKey: 'forward' },
    ],
  },
  {
    titleKey: 'general',
    shortcuts: [
      { keys: ['?'], descKey: 'help' },
      { keys: ['click'], descKey: 'altClickDeselect' },
    ],
  },
];

/**
 * Modal overlay listing every keyboard / mouse shortcut on the graph page.
 *
 * Triggered by `?` from the canvas (see `Graph3D.onShowHelp`). Escape and
 * clicking the scrim both close. We never trap Tab — the matrix is short
 * and screen-reader users get a single dismiss path via Esc.
 */
export function GraphHelpOverlay({ open, onClose }: Props) {
  const { t } = useTranslation();
  const titleId = useId();
  const descId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);

  // Escape closes. Bind on `window` because focus might land anywhere
  // inside the dialog (the close button, a kbd cell focused via TAB, etc).
  // Effect explicitly depends on `open` — we don't want a stale listener
  // firing after the overlay has been unmounted.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  // Move focus to the dialog on open so SR users hear the heading and
  // keyboard users have a sensible Tab origin. We don't use autoFocus on
  // the Close button — that would announce "Close button" as the first
  // thing, which is the *exit*, not the content.
  useEffect(() => {
    if (open) dialogRef.current?.focus();
  }, [open]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      {/* biome-ignore lint/a11y/noStaticElementInteractions: scrim is a click target, not an interactive control */}
      <div
        role="presentation"
        data-testid="graph-help-scrim"
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
        onClick={onClose}
      />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descId}
        tabIndex={-1}
        className="relative w-full max-w-xl max-h-[90vh] overflow-y-auto bg-card border border-border rounded-xl shadow-2xl focus:outline-none"
      >
        <div className="p-6 space-y-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <h2 id={titleId} className="text-lg font-bold text-foreground">
                {t('graph.help.title')}
              </h2>
              <p id={descId} className="text-xs text-muted-foreground mt-1">
                {t('graph.help.subtitle')}
              </p>
            </div>
            <Button variant="ghost" size="sm" onClick={onClose} aria-label={t('graph.help.close')}>
              ×
            </Button>
          </div>

          <div className="space-y-5">
            {SECTIONS.map((section) => (
              <section key={section.titleKey} aria-labelledby={`${titleId}-${section.titleKey}`}>
                <h3
                  id={`${titleId}-${section.titleKey}`}
                  className="text-xs uppercase tracking-wider text-muted-foreground mb-2"
                >
                  {t(`graph.help.sections.${section.titleKey}`)}
                </h3>
                <dl className="space-y-1.5">
                  {section.shortcuts.map((s) => (
                    <div key={s.descKey} className="flex items-baseline gap-3 text-sm">
                      <dt className="flex items-center gap-1 shrink-0 min-w-[6.5rem]">
                        {s.keys.map((k) => (
                          <kbd
                            key={`${s.descKey}-${k}`}
                            className="inline-flex items-center justify-center min-w-[1.5rem] h-6 px-1.5 rounded border border-border bg-muted text-xs font-mono text-foreground"
                          >
                            {k}
                          </kbd>
                        ))}
                      </dt>
                      <dd className="text-muted-foreground">{t(`graph.help.keys.${s.descKey}`)}</dd>
                    </div>
                  ))}
                </dl>
              </section>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
