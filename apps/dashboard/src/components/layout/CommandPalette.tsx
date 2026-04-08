import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';

const NAV_COMMANDS = [
  { to: 'graph', icon: '◈', labelKey: 'nav.graph' },
  { to: 'memories', icon: '◉', labelKey: 'nav.memories' },
  { to: 'timeline', icon: '◎', labelKey: 'nav.timeline' },
  { to: 'feed', icon: '◊', labelKey: 'nav.feed' },
  { to: 'explore', icon: '⬡', labelKey: 'nav.explore' },
  { to: 'intentions', icon: '◌', labelKey: 'nav.intentions' },
  { to: 'stats', icon: '▣', labelKey: 'nav.stats' },
  { to: 'settings', icon: '⚙', labelKey: 'nav.settings' },
] as const;

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [query, setQuery] = useState('');

  useEffect(() => {
    if (open) {
      dialogRef.current?.showModal();
      setQuery('');
    } else {
      dialogRef.current?.close();
    }
  }, [open]);

  const filtered = NAV_COMMANDS.filter(
    (c) => !query || t(c.labelKey).toLowerCase().includes(query.toLowerCase()),
  );

  const runCommand = useCallback(
    (to: string) => {
      onClose();
      navigate(to);
    },
    [navigate, onClose],
  );

  return (
    <dialog
      ref={dialogRef}
      onClose={onClose}
      className="fixed inset-0 z-50 m-0 p-0 w-full h-full max-w-none max-h-none bg-transparent backdrop:bg-black/40 dark:backdrop:bg-black/60"
      aria-label={t('a11y.commandPalette')}
    >
      <div className="flex items-start justify-center pt-[20vh] w-full h-full">
        <button
          type="button"
          className="fixed inset-0 cursor-default bg-transparent border-none"
          onClick={onClose}
          aria-label={t('a11y.closeCommandPalette')}
          tabIndex={-1}
        />
        <search className="relative glass-panel rounded-2xl w-full max-w-md p-4 space-y-3 z-10">
          <input
            type="text"
            placeholder={t('a11y.searchCommands')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="w-full bg-accent border border-border rounded-xl px-4 py-2.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            autoFocus
            onKeyDown={(e) => {
              if (e.key === 'Escape') onClose();
              if (e.key === 'Enter' && filtered.length > 0) {
                runCommand(filtered[0].to);
              }
            }}
            aria-label={t('a11y.searchCommands')}
          />
          <ul className="space-y-1" role="listbox">
            {filtered.map((cmd) => (
              <li key={cmd.to} role="option" aria-selected={false}>
                <button
                  type="button"
                  onClick={() => runCommand(cmd.to)}
                  className="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm text-muted-foreground hover:text-foreground hover:bg-accent transition"
                >
                  <span aria-hidden="true">{cmd.icon}</span>
                  <span>{t(cmd.labelKey)}</span>
                </button>
              </li>
            ))}
          </ul>
        </search>
      </div>
    </dialog>
  );
}
