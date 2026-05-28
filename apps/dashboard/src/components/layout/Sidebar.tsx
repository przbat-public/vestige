import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router';
import { useShallow } from 'zustand/react/shallow';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useWebSocket } from '@/stores/websocket';
import { DensityToggle } from './DensityToggle';
import { LanguageSwitcher } from './LanguageSwitcher';
import { NAV_SECTIONS } from './nav-sections';
import { ThemeToggle } from './ThemeToggle';

/** Highest count we render verbatim; anything above shows as "99+". */
const REVIEW_BADGE_CAP = 99;

interface SidebarProps {
  onNavigate?: () => void;
  onOpenCommandPalette?: () => void;
}

export function Sidebar({ onNavigate, onOpenCommandPalette }: SidebarProps) {
  const { t } = useTranslation();
  // Subscribe ONLY to the three fields we render. Without `useShallow` the
  // bare `useWebSocket()` call returned the full store, which re-rendered the
  // sidebar (16 NavLinks) on every event push and heartbeat — the WS stream
  // produces several updates per second under normal traffic.
  const { connected, memoryCount, avgRetention } = useWebSocket(
    useShallow((s) => ({
      connected: s.connected,
      memoryCount: s.memoryCount,
      avgRetention: s.avgRetention,
    })),
  );

  // Reuse the existing `stats` query (already loaded by GraphPage / Layout)
  // so the badge piggybacks on cached data and adds zero network traffic in
  // the common case. WebSocket invalidation keeps it fresh after reviews.
  const { data: stats } = useQuery({
    queryKey: queryKeys.stats,
    queryFn: api.stats,
    staleTime: 60_000,
  });
  const dueCount = stats?.dueForReview ?? 0;
  const dueLabel = dueCount > REVIEW_BADGE_CAP ? `${REVIEW_BADGE_CAP}+` : String(dueCount);

  return (
    <nav
      className="flex flex-col h-full w-56 py-4 z-20 bg-sidebar border-r border-sidebar-border"
      aria-label={t('a11y.mainNavigation')}
    >
      <div className="px-4 mb-6">
        <h1 className="text-lg font-bold text-foreground">{t('app.name')}</h1>
        <p className="text-xs text-muted-foreground mt-0.5">{t('app.tagline')}</p>
      </div>

      <div className="flex-1 px-2 space-y-3 overflow-y-auto">
        {NAV_SECTIONS.map((section) => {
          const headingId = `sidebar-section-${section.id}`;
          return (
            <section key={section.id} aria-labelledby={headingId}>
              <h2
                id={headingId}
                className="px-3 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/70"
              >
                {t(section.labelKey)}
              </h2>
              <ul className="space-y-0.5">
                {section.items.map((item) => {
                  const showDueBadge = item.to === 'review' && dueCount > 0;
                  return (
                    <li key={item.to}>
                      <NavLink
                        to={item.to}
                        onClick={onNavigate}
                        className={({ isActive }) =>
                          `flex items-center justify-between gap-2 px-3 py-2 rounded-lg text-sm transition-all ${
                            isActive
                              ? 'bg-primary/10 text-primary font-medium nav-active-border'
                              : 'text-muted-foreground hover:text-foreground hover:bg-accent'
                          }`
                        }
                      >
                        {({ isActive }) => (
                          <>
                            <span className="truncate">
                              {t(item.labelKey)}
                              {isActive && <span className="sr-only">{t('a11y.currentPage')}</span>}
                            </span>
                            {showDueBadge && (
                              // Visible badge is the count, hidden text is the
                              // verbose plural-aware aria announcement so screen
                              // readers say "3 memories due for review" rather
                              // than a bare numeric. `role="status"` makes
                              // `aria-label` valid on the otherwise
                              // non-interactive span.
                              <span
                                role="status"
                                className="shrink-0 px-1.5 py-0.5 rounded-full text-[10px] font-medium tabular-nums bg-warning/20 text-warning"
                              >
                                <span aria-hidden="true">{dueLabel}</span>
                                <span className="sr-only">{t('nav.reviewDueAria', { count: dueCount })}</span>
                              </span>
                            )}
                          </>
                        )}
                      </NavLink>
                    </li>
                  );
                })}
              </ul>
            </section>
          );
        })}
      </div>

      <div className="px-3 py-3 border-t border-sidebar-border space-y-2">
        <div className="flex items-center justify-between">
          <LanguageSwitcher />
          <div className="flex items-center gap-1">
            <DensityToggle />
            <ThemeToggle />
          </div>
        </div>

        <div className="flex items-center gap-2 text-xs">
          <span
            className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
              connected ? 'bg-success animate-pulse-glow' : 'bg-danger'
            }`}
            role="status"
            aria-label={connected ? t('a11y.connected') : t('a11y.disconnected')}
          />
          <span className="text-muted-foreground truncate">
            {connected ? t('status.memoriesCount', { count: memoryCount }) : t('status.disconnected')}
          </span>
        </div>
        {connected && (
          <div className="text-xs text-muted-foreground pl-3.5">
            {t('status.avgRetention', { value: (avgRetention * 100).toFixed(0) })}
          </div>
        )}

        {onOpenCommandPalette && (
          <button
            type="button"
            onClick={onOpenCommandPalette}
            className="w-full flex items-center gap-2 px-2 py-1.5 rounded-lg bg-accent border border-border text-xs text-muted-foreground hover:text-foreground hover:bg-accent/80 transition"
          >
            <kbd className="text-xs">⌘K</kbd>
            <span>{t('a11y.commandPalette')}</span>
          </button>
        )}
      </div>
    </nav>
  );
}
