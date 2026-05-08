import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { api } from '@/stores/api';
import { toast } from '@/stores/toast';

/**
 * A command is either a navigation jump (no side effect, instant) or an
 * action (async, can show a toast, can be destructive). Discriminated union
 * lets the renderer treat them uniformly while keeping the right handler
 * shape per kind.
 */
type CommandItem =
  | {
      kind: 'navigate';
      id: string;
      icon: string;
      labelKey: string;
      to: string;
    }
  | {
      kind: 'action';
      id: string;
      icon: string;
      labelKey: string;
      /** Returns a translation key for the success toast. */
      run: () => Promise<string>;
      /** When true, runs `confirm(...)` with `confirmKey` before executing. */
      destructive?: boolean;
      confirmKey?: string;
    };

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const [runningId, setRunningId] = useState<string | null>(null);

  const commands: CommandItem[] = useMemo(
    () => [
      // ===== Navigation =====
      { kind: 'navigate', id: 'nav-graph', icon: '◈', labelKey: 'nav.graph', to: 'graph' },
      { kind: 'navigate', id: 'nav-memories', icon: '◉', labelKey: 'nav.memories', to: 'memories' },
      { kind: 'navigate', id: 'nav-review', icon: '✦', labelKey: 'nav.review', to: 'review' },
      { kind: 'navigate', id: 'nav-briefing', icon: '☀', labelKey: 'nav.briefing', to: 'briefing' },
      { kind: 'navigate', id: 'nav-timeline', icon: '◎', labelKey: 'nav.timeline', to: 'timeline' },
      { kind: 'navigate', id: 'nav-feed', icon: '◊', labelKey: 'nav.feed', to: 'feed' },
      { kind: 'navigate', id: 'nav-explore', icon: '⬡', labelKey: 'nav.explore', to: 'explore' },
      { kind: 'navigate', id: 'nav-intentions', icon: '◌', labelKey: 'nav.intentions', to: 'intentions' },
      { kind: 'navigate', id: 'nav-stats', icon: '▣', labelKey: 'nav.stats', to: 'stats' },
      { kind: 'navigate', id: 'nav-settings', icon: '⚙', labelKey: 'nav.settings', to: 'settings' },
      // ===== Actions =====
      {
        kind: 'action',
        id: 'action-dream',
        icon: '☾',
        labelKey: 'commands.dream',
        run: async () => {
          await api.dream();
          return 'commands.dreamSuccess';
        },
      },
      {
        kind: 'action',
        id: 'action-consolidate',
        icon: '⟳',
        labelKey: 'commands.consolidate',
        run: async () => {
          await api.consolidate();
          return 'commands.consolidateSuccess';
        },
      },
      {
        kind: 'action',
        id: 'action-reflect',
        icon: '✦',
        labelKey: 'commands.reflect',
        run: async () => {
          await api.reflect(undefined, 'standard');
          return 'commands.reflectSuccess';
        },
      },
      {
        kind: 'action',
        id: 'action-confidence-audit',
        icon: '◆',
        labelKey: 'commands.confidenceAudit',
        run: async () => {
          await api.confidence('audit', undefined, 20);
          return 'commands.confidenceAuditSuccess';
        },
      },
      {
        kind: 'action',
        id: 'action-regenerate-embeddings',
        icon: '∿',
        labelKey: 'commands.regenerateEmbeddings',
        run: async () => {
          await api.maintenance.regenerateEmbeddings({ force: false });
          return 'commands.regenerateEmbeddingsSuccess';
        },
      },
      {
        kind: 'action',
        id: 'action-backup',
        icon: '☁︎',
        labelKey: 'commands.backup',
        run: async () => {
          await api.maintenance.backup();
          return 'commands.backupSuccess';
        },
      },
    ],
    [],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((c) => t(c.labelKey).toLowerCase().includes(q));
  }, [commands, query, t]);

  // Keep activeIndex in bounds when filter shrinks the list.
  useEffect(() => {
    if (activeIndex >= filtered.length) setActiveIndex(0);
  }, [filtered.length, activeIndex]);

  useEffect(() => {
    if (open) {
      dialogRef.current?.showModal();
      setQuery('');
      setActiveIndex(0);
    } else {
      dialogRef.current?.close();
    }
  }, [open]);

  const runCommand = useCallback(
    async (cmd: CommandItem) => {
      if (cmd.kind === 'navigate') {
        onClose();
        navigate(cmd.to);
        return;
      }
      // Action path
      if (cmd.destructive && cmd.confirmKey) {
        // Native confirm — synchronous, blocks the UI thread, but is
        // accessible (announced as alert dialog by all major screen readers)
        // and requires no extra component. Adequate for low-frequency
        // destructive maintenance actions.
        const confirmed = window.confirm(t(cmd.confirmKey));
        if (!confirmed) return;
      }
      setRunningId(cmd.id);
      try {
        const successKey = await cmd.run();
        toast(t(successKey), 'success');
        onClose();
      } catch (err) {
        const message = err instanceof Error ? err.message : t('common.error');
        toast(message, 'error');
      } finally {
        setRunningId(null);
      }
    },
    [navigate, onClose, t],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Escape') {
        onClose();
        return;
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, filtered.length - 1));
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
        return;
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        const target = filtered[activeIndex];
        if (target) runCommand(target);
      }
    },
    [activeIndex, filtered, onClose, runCommand],
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
            onKeyDown={onKeyDown}
            aria-label={t('a11y.searchCommands')}
            aria-controls="command-palette-list"
            aria-activedescendant={filtered[activeIndex]?.id ?? undefined}
            role="combobox"
            aria-expanded="true"
            aria-autocomplete="list"
          />
          <ul
            id="command-palette-list"
            className="space-y-1 max-h-[50vh] overflow-y-auto"
            role="listbox"
          >
            {filtered.length === 0 && (
              <li className="text-xs text-muted-foreground px-3 py-2">{t('common.noResults')}</li>
            )}
            {filtered.map((cmd, i) => {
              const isActive = i === activeIndex;
              const isRunning = cmd.kind === 'action' && runningId === cmd.id;
              const disabled = runningId !== null && !isRunning;
              return (
                <li
                  key={cmd.id}
                  id={cmd.id}
                  role="option"
                  aria-selected={isActive}
                >
                  <button
                    type="button"
                    onClick={() => runCommand(cmd)}
                    onMouseEnter={() => setActiveIndex(i)}
                    disabled={disabled}
                    className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition ${
                      isActive
                        ? 'bg-accent text-foreground'
                        : 'text-muted-foreground hover:text-foreground hover:bg-accent'
                    } ${disabled ? 'opacity-50 cursor-not-allowed' : ''}`}
                  >
                    <span aria-hidden="true">{cmd.icon}</span>
                    <span className="flex-1 text-left">{t(cmd.labelKey)}</span>
                    {cmd.kind === 'action' && (
                      <span
                        className="text-[10px] uppercase tracking-wider opacity-60"
                        aria-hidden="true"
                      >
                        {isRunning ? t('common.loading') : t('commands.actionBadge')}
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        </search>
      </div>
    </dialog>
  );
}
