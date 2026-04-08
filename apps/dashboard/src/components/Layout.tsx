import { useEffect, useState } from 'react';
import { Outlet } from 'react-router';
import { useTranslation } from 'react-i18next';
import { Sidebar } from '@/components/layout/Sidebar';
import { CommandPalette } from '@/components/layout/CommandPalette';
import { RouteAnnouncer } from '@/components/RouteAnnouncer';

export function Layout() {
  const { t } = useTranslation();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [cmdOpen, setCmdOpen] = useState(false);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setCmdOpen((prev) => !prev);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
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
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
              <path d="M3 12h18M3 6h18M3 18h18" />
            </svg>
          </button>
          <span className="text-sm text-foreground font-bold">{t('app.name')}</span>
        </div>
        <Outlet />
      </main>

      {/* Command palette */}
      <CommandPalette open={cmdOpen} onClose={() => setCmdOpen(false)} />
    </div>
  );
}
