import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { useWebSocket } from '@/stores/websocket';
import { LanguageSwitcher } from './LanguageSwitcher';
import { ThemeToggle } from './ThemeToggle';

const NAV_ITEMS = [
  { to: 'graph', labelKey: 'nav.graph' },
  { to: 'memories', labelKey: 'nav.memories' },
  { to: 'review', labelKey: 'nav.review' },
  { to: 'briefing', labelKey: 'nav.briefing' },
  { to: 'timeline', labelKey: 'nav.timeline' },
  { to: 'feed', labelKey: 'nav.feed' },
  { to: 'explore', labelKey: 'nav.explore' },
  { to: 'intentions', labelKey: 'nav.intentions' },
  { to: 'stats', labelKey: 'nav.stats' },
  { to: 'settings', labelKey: 'nav.settings' },
  { to: 'tutorial', labelKey: 'nav.tutorial' },
] as const;

/** Highest count we render verbatim; anything above shows as "99+". */
const REVIEW_BADGE_CAP = 99;

interface SidebarProps {
  onNavigate?: () => void;
  onOpenCommandPalette?: () => void;
}

export function Sidebar({ onNavigate, onOpenCommandPalette }: SidebarProps) {
  const { t } = useTranslation();
  const { connected, memoryCount, avgRetention } = useWebSocket();

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

      <div className="flex-1 px-2 space-y-0.5 overflow-y-auto">
        {NAV_ITEMS.map((item) => {
          const showDueBadge = item.to === 'review' && dueCount > 0;
          return (
            <NavLink
              key={item.to}
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
                    {isActive && <span className="sr-only">(current page)</span>}
                  </span>
                  {showDueBadge && (
                    // Visible badge is the count, hidden text is the
                    // verbose plural-aware aria announcement so screen
                    // readers say "3 memories due for review" rather than
                    // a bare numeric. `role="status"` makes `aria-label`
                    // valid on the otherwise non-interactive span.
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
          );
        })}
      </div>

      <div className="px-3 py-3 border-t border-sidebar-border space-y-2">
        <div className="flex items-center justify-between">
          <LanguageSwitcher />
          <ThemeToggle />
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
