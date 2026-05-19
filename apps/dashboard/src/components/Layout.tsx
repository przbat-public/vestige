import { lazy, Suspense, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Outlet } from 'react-router';
import { CommandPalette } from '@/components/layout/CommandPalette';
import { KeyboardShortcutsDialog } from '@/components/layout/KeyboardShortcutsDialog';
import { Sidebar } from '@/components/layout/Sidebar';
import { RouteAnnouncer } from '@/components/RouteAnnouncer';
import { EVENT, track } from '@/stores/telemetry';

// Lazy-loaded — the dialog pulls react-hook-form + zod which only matter
// once the user actually opens "Add memory". Keeps initial Layout chunk
// lean.
const AddMemoryDialog = lazy(() =>
  import('@/components/memories/AddMemoryDialog').then((m) => ({ default: m.AddMemoryDialog })),
);

export function Layout() {
  const { t } = useTranslation();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [cmdOpen, setCmdOpen] = useState(false);
  const [addMemoryOpen, setAddMemoryOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  useEffect(() => {
    // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: keyboard router intentionally enumerates each chord linearly so reviewers can audit shortcut precedence at a glance; splitting per-chord would obscure the order
    function onKey(e: KeyboardEvent) {
      // `?` / Cmd+/ — open the keyboard shortcuts cheat sheet. We check
      // this BEFORE the typing guard because the user pressing `?` while
      // in an input is much more likely to mean "I want the cheat sheet"
      // than wanting to literally type "?" — but we keep the typing
      // bail-out for the bare `?` variant. Cmd+/ is the JetBrains/VS Code
      // convention for "help".
      if ((e.metaKey || e.ctrlKey) && e.key === '/') {
        e.preventDefault();
        setShortcutsOpen((prev) => {
          if (!prev) track(EVENT.shortcuts_dialog_open, { trigger: 'cmd_slash' });
          return !prev;
        });
        return;
      }

      const target = e.target as HTMLElement | null;
      const typing =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable === true;

      // Bare `?` only when the user isn't typing — same guard rationale
      // as the `'n'` handler below.
      if (e.key === '?' && !typing) {
        e.preventDefault();
        setShortcutsOpen((prev) => {
          if (!prev) track(EVENT.shortcuts_dialog_open, { trigger: 'question_mark' });
          return !prev;
        });
        return;
      }

      // Don't intercept ⌘K / ⌘N when the user is typing in an editable
      // field — `'n'` would otherwise trigger every time they type a letter
      // 'n' inside Search or any input. ⌘K is fine because the meta key
      // disambiguates from typing "k".
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setCmdOpen((prev) => !prev);
        return;
      }
      // ⌘N / Ctrl+N → open Add Memory. Browsers reserve ⌘N for "new
      // window" but that hotkey isn't reachable from page JS anyway, so
      // we layer our intent on top — works inside the dashboard tab,
      // doesn't fight the browser.
      if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
        e.preventDefault();
        setAddMemoryOpen(true);
        track(EVENT.add_memory_open, { trigger: 'cmd_n' });
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  // Custom event lets empty-state CTAs (e.g. on MemoriesPage) open the
  // Add Memory dialog without prop-drilling through every page. The
  // dialog state lives here because the ⌘N shortcut also lives here —
  // single source of truth wins over multiple lazy clones.
  useEffect(() => {
    function onOpenAddMemory() {
      setAddMemoryOpen(true);
      track(EVENT.add_memory_open, { trigger: 'event' });
    }
    window.addEventListener('vestige:open-add-memory', onOpenAddMemory);
    return () => window.removeEventListener('vestige:open-add-memory', onOpenAddMemory);
  }, []);

  return (
    <div className="flex h-screen w-screen overflow-hidden relative">
      {/* Skip link */}
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:fixed focus:top-4 focus:left-4 focus:z-[100] focus:px-4 focus:py-2 focus:rounded-xl focus:bg-primary focus:text-primary-foreground focus:text-sm focus:font-medium focus:shadow-lg"
      >
        {t('a11y.skipToContent')}
      </a>

      {/* Route announcer for screen readers */}
      <RouteAnnouncer />

      {/* Ambient background orbs (dark mode only) */}
      <div className="ambient-orb ambient-orb-1" aria-hidden="true" />
      <div className="ambient-orb ambient-orb-2" aria-hidden="true" />
      <div className="ambient-orb ambient-orb-3" aria-hidden="true" />

      {/* Desktop sidebar */}
      <div className="hidden md:flex flex-shrink-0">
        <Sidebar onOpenCommandPalette={() => setCmdOpen(true)} />
      </div>

      {/* Mobile sidebar overlay */}
      {mobileOpen && (
        <div className="fixed inset-0 z-40 md:hidden">
          <button
            type="button"
            className="absolute inset-0 bg-black/40 dark:bg-black/60 cursor-default"
            onClick={() => setMobileOpen(false)}
            aria-label={t('a11y.closeNavigation')}
          />
          <div className="relative h-full w-56">
            <Sidebar
              onNavigate={() => setMobileOpen(false)}
              onOpenCommandPalette={() => {
                setMobileOpen(false);
                setCmdOpen(true);
              }}
            />
          </div>
        </div>
      )}

      {/* Main content */}
      <main id="main-content" tabIndex={-1} className="flex-1 min-w-0 overflow-y-auto relative z-10 focus:outline-none">
        {/* Mobile header */}
        <div className="md:hidden flex items-center gap-3 p-3 border-b border-border bg-background">
          <button
            type="button"
            onClick={() => setMobileOpen(true)}
            className="text-muted-foreground hover:text-foreground p-1"
            aria-label={t('a11y.openNavigation')}
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              aria-hidden="true"
            >
              <path d="M3 12h18M3 6h18M3 18h18" />
            </svg>
          </button>
          <span className="text-sm text-foreground font-bold">{t('app.name')}</span>
        </div>
        <Outlet />
      </main>

      {/* Floating "Add memory" button — visible on every page. The dashboard
          was read-only for the primary action (creating memories) before
          this; the FAB is the dashboard's first-class write affordance.
          Positioned bottom-right so it doesn't compete with page chrome,
          and offset by safe-area-inset on iOS. */}
      <button
        type="button"
        onClick={() => {
          setAddMemoryOpen(true);
          track(EVENT.add_memory_open, { trigger: 'fab' });
        }}
        className="fixed bottom-6 right-6 z-30 w-14 h-14 rounded-full bg-primary text-primary-foreground shadow-2xl hover:scale-105 active:scale-95 transition-transform flex items-center justify-center text-2xl font-light focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
        style={{ marginBottom: 'env(safe-area-inset-bottom, 0px)' }}
        aria-label={t('addMemory.fabLabel')}
        title={t('addMemory.fabTitle')}
      >
        <span aria-hidden="true">+</span>
      </button>

      {/* Command palette */}
      <CommandPalette open={cmdOpen} onClose={() => setCmdOpen(false)} />

      {/* Keyboard shortcuts cheat sheet — ? or ⌘/ */}
      <KeyboardShortcutsDialog open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />

      {/* Add memory dialog — lazy chunk loads on first open */}
      <Suspense fallback={null}>
        {addMemoryOpen && <AddMemoryDialog open={addMemoryOpen} onClose={() => setAddMemoryOpen(false)} />}
      </Suspense>
    </div>
  );
}
