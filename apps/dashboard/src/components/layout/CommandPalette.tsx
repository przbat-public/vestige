import { useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { useDebounce } from '@/hooks/use-debounce';
import { api } from '@/stores/api';
import { confirm } from '@/stores/confirm';
import { useDialogStore } from '@/stores/dialogs';
import { EVENT, track } from '@/stores/telemetry';
import { toast } from '@/stores/toast';

/**
 * A command palette entry. Three kinds, each with a single canonical handler
 * shape so the renderer can stay flat. Navigation jumps to a route, actions
 * run a maintenance call, and memory hits jump to the Memories page with
 * the chosen memory pre-selected via a window event (see Layout listener).
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
    }
  | {
      kind: 'memory';
      id: string; // synthetic for activedescendant; format `mem-<memoryId>`
      memoryId: string;
      icon: string;
      label: string; // pre-translated (snippet from server) — bypasses i18n
      nodeType: string;
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

  // Static commands (nav + actions); memory hits are merged in below. We
  // narrow off the 'memory' variant here so `c.labelKey` is reachable
  // without a type guard in the filter.
  const commands: Exclude<CommandItem, { kind: 'memory' }>[] = useMemo(
    () => [
      // ===== Navigation =====
      { kind: 'navigate', id: 'nav-graph', icon: '◈', labelKey: 'nav.graph', to: 'graph' },
      { kind: 'navigate', id: 'nav-memories', icon: '◉', labelKey: 'nav.memories', to: 'memories' },
      { kind: 'navigate', id: 'nav-review', icon: '✦', labelKey: 'nav.review', to: 'review' },
      { kind: 'navigate', id: 'nav-briefing', icon: '☀', labelKey: 'nav.briefing', to: 'briefing' },
      { kind: 'navigate', id: 'nav-timeline', icon: '◎', labelKey: 'nav.timeline', to: 'timeline' },
      { kind: 'navigate', id: 'nav-feed', icon: '◊', labelKey: 'nav.feed', to: 'feed' },
      { kind: 'navigate', id: 'nav-explore', icon: '⬡', labelKey: 'nav.explore', to: 'explore' },
      // Discovery → understanding → planning. Five pages were missing
      // from the palette even though the sidebar routes them all; power
      // users live behind Cmd+K, so a gap here means the page may as
      // well not exist for the keyboard.
      { kind: 'navigate', id: 'nav-hubs', icon: '⌬', labelKey: 'nav.hubs', to: 'hubs' },
      { kind: 'navigate', id: 'nav-insights', icon: '◇', labelKey: 'nav.insights', to: 'insights' },
      { kind: 'navigate', id: 'nav-decisions', icon: '◐', labelKey: 'nav.decisions', to: 'decisions' },
      { kind: 'navigate', id: 'nav-temporal', icon: '⌛', labelKey: 'nav.temporal', to: 'temporal' },
      { kind: 'navigate', id: 'nav-reasoning', icon: '⚖', labelKey: 'nav.reasoning', to: 'reasoning' },
      { kind: 'navigate', id: 'nav-intentions', icon: '◌', labelKey: 'nav.intentions', to: 'intentions' },
      { kind: 'navigate', id: 'nav-stats', icon: '▣', labelKey: 'nav.stats', to: 'stats' },
      { kind: 'navigate', id: 'nav-settings', icon: '⚙', labelKey: 'nav.settings', to: 'settings' },
      // Tutorial is on the router (App.tsx) and in the sidebar but
      // never made it into the keyboard surface — meaning anyone who
      // lives in Cmd+K couldn't reach "How does this app work?"
      // without grabbing the mouse. Place it last in nav so it
      // doesn't crowd the daily-driver routes.
      { kind: 'navigate', id: 'nav-tutorial', icon: '?', labelKey: 'nav.tutorial', to: 'tutorial' },
      // ===== Actions =====
      // "Add memory" lives in the palette because Cmd+K is the only
      // keyboard surface power users reach for outside individual
      // pages. Cmd+N is the dedicated shortcut, but adding it here
      // means a single discoverable surface ("what can I do?")
      // covers both navigation and the dashboard's primary write
      // action. Action is synchronous from the palette's POV — it
      // just flips the shared dialog store flag and closes; the
      // dialog UI itself opens via Layout's subscription.
      {
        kind: 'action',
        id: 'action-add-memory',
        icon: '+',
        labelKey: 'commands.addMemory',
        run: async () => {
          useDialogStore.getState().openAddMemory('palette');
          return '';
        },
      },
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

  // ===== Live memory search =====
  // When the user types ≥2 characters, run `api.search` so the palette
  // doubles as a global content search — type "checkout" and jump
  // straight to that memory. Debounced (180ms) to avoid hammering the
  // engine for every keystroke; capped at 5 hits so the panel doesn't
  // overflow the viewport.
  const debouncedQuery = useDebounce(query.trim(), 180);
  const searchEnabled = open && debouncedQuery.length >= 2;
  const { data: searchData, isFetching: searchLoading } = useQuery({
    queryKey: ['command-palette-search', debouncedQuery],
    queryFn: () => api.search(debouncedQuery, 5),
    enabled: searchEnabled,
    // Stale-while-revalidate: keep previous hits visible while we fetch
    // the next slice so the list doesn't flash empty between keystrokes.
    staleTime: 30_000,
  });

  const memoryHits: CommandItem[] = useMemo(() => {
    if (!searchEnabled || !searchData?.results) return [];
    return searchData.results.slice(0, 5).map((m) => ({
      kind: 'memory' as const,
      id: `mem-${m.id}`,
      memoryId: m.id,
      // Neutral glyph — node type is shown as a trailing badge so we
      // don't need to encode it twice.
      icon: '◉',
      // Slice rather than CSS-truncate so the activedescendant label
      // matches what screen readers announce.
      label: m.content.length > 70 ? `${m.content.slice(0, 70)}…` : m.content,
      nodeType: m.nodeType,
    }));
  }, [searchData, searchEnabled]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    // Commands filter on the translated label (so "Briefing" matches in
    // both languages). Memory hits already encode the match — they were
    // returned by hybrid search — so we just append them.
    const matchedCommands = q ? commands.filter((c) => t(c.labelKey).toLowerCase().includes(q)) : commands;
    return [...matchedCommands, ...memoryHits];
  }, [commands, memoryHits, query, t]);

  // Keep activeIndex in bounds when filter shrinks the list.
  useEffect(() => {
    if (activeIndex >= filtered.length) setActiveIndex(0);
  }, [filtered.length, activeIndex]);

  useEffect(() => {
    if (open) {
      dialogRef.current?.showModal();
      setQuery('');
      setActiveIndex(0);
      track(EVENT.command_palette_open);
    } else {
      dialogRef.current?.close();
    }
  }, [open]);

  // Action path is the most branchy: confirm, await, toast, finally. Pulled
  // out so `runCommand` itself stays a short dispatcher and clears Biome's
  // complexity threshold.
  const runAction = useCallback(
    async (cmd: Extract<CommandItem, { kind: 'action' }>) => {
      if (cmd.destructive && cmd.confirmKey) {
        // Themed alertdialog (`stores/confirm.ts`) — focuses cancel by
        // default, accepts ESC/Enter, plays nicely with Playwright.
        const confirmed = await confirm({
          message: t(cmd.confirmKey),
          destructive: true,
        });
        if (!confirmed) return;
      }
      setRunningId(cmd.id);
      try {
        const successKey = await cmd.run();
        // Some actions are silent on success — they open a dialog
        // and the dialog itself is the user feedback. An empty
        // string returned from `run()` opts out of the toast.
        if (successKey) toast(t(successKey), 'success');
        onClose();
      } catch (err) {
        const message = err instanceof Error ? err.message : t('common.error');
        toast(message, 'error');
      } finally {
        setRunningId(null);
      }
    },
    [onClose, t],
  );

  const runCommand = useCallback(
    async (cmd: CommandItem) => {
      track(EVENT.command_palette_select, { id: cmd.id, kind: cmd.kind });
      if (cmd.kind === 'navigate') {
        onClose();
        navigate(cmd.to);
        return;
      }
      if (cmd.kind === 'memory') {
        // Stash the requested memory id in the shared dialog store *before*
        // navigating. `MemoriesPage` reads the slot in a `useEffect` on
        // mount/store-change and immediately clears it, so the open is
        // idempotent — no timeouts, no missed-mount races, no listener
        // teardown problems.
        useDialogStore.getState().requestSelectMemory(cmd.memoryId);
        onClose();
        navigate('/memories');
        return;
      }
      await runAction(cmd);
    },
    [navigate, onClose, runAction],
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
          {/* biome-ignore lint/a11y/noNoninteractiveElementToInteractiveRole: WAI-ARIA combobox+listbox pattern requires role="listbox" on the option container */}
          <ul id="command-palette-list" className="space-y-1 max-h-[50vh] overflow-y-auto" role="listbox">
            {filtered.length === 0 && !searchLoading && (
              <li className="text-xs text-muted-foreground px-3 py-2">{t('common.noResults')}</li>
            )}
            {filtered.map((cmd, i) => {
              const isActive = i === activeIndex;
              const isRunning = cmd.kind === 'action' && runningId === cmd.id;
              const disabled = runningId !== null && !isRunning;
              // Section header above the first memory hit. Cheaper than
              // splitting the list into two arrays — keeps activeIndex
              // (and arrow navigation) coupled to a single source of truth.
              const previous = filtered[i - 1];
              const showMemoryHeader = cmd.kind === 'memory' && previous?.kind !== 'memory';
              return (
                <div key={cmd.id}>
                  {showMemoryHeader && (
                    <div
                      // Decorative grouping label, not an interactive option.
                      className="text-[10px] uppercase tracking-wider text-muted-foreground/70 px-3 pt-3 pb-1"
                      aria-hidden="true"
                    >
                      {t('commands.memoryResults', 'Memories')}
                    </div>
                  )}
                  {/* biome-ignore lint/a11y/useFocusableInteractive: combobox keeps focus on the input and routes selection through aria-activedescendant; options must not be tabbable */}
                  {/* biome-ignore lint/a11y/noNoninteractiveElementToInteractiveRole: WAI-ARIA listbox children must use role="option" */}
                  <li id={cmd.id} role="option" aria-selected={isActive}>
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
                      <span className="flex-1 text-left truncate">
                        {cmd.kind === 'memory' ? cmd.label : t(cmd.labelKey)}
                      </span>
                      {cmd.kind === 'action' && (
                        <span className="text-[10px] uppercase tracking-wider opacity-60" aria-hidden="true">
                          {isRunning ? t('common.loading') : t('commands.actionBadge')}
                        </span>
                      )}
                      {cmd.kind === 'memory' && (
                        <span className="text-[10px] uppercase tracking-wider opacity-60" aria-hidden="true">
                          {cmd.nodeType}
                        </span>
                      )}
                    </button>
                  </li>
                </div>
              );
            })}
            {searchLoading && (
              <li className="text-[11px] text-muted-foreground px-3 py-1.5 italic" aria-live="polite">
                {t('common.searching', 'Searching…')}
              </li>
            )}
          </ul>
        </search>
      </div>
    </dialog>
  );
}
