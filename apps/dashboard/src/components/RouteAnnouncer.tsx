import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation } from 'react-router';

// Keep this list in sync with `Sidebar.NAV_ITEMS` — every route the user can
// reach from the nav must be announced here, otherwise screen-reader users
// don't get any feedback when navigating to it. Missing `/review` and
// `/briefing` was a regression after those pages were added.
const ROUTE_TITLES: Record<string, string> = {
  '/graph': 'nav.graph',
  '/memories': 'nav.memories',
  '/review': 'nav.review',
  '/briefing': 'nav.briefing',
  '/timeline': 'nav.timeline',
  '/feed': 'nav.feed',
  '/explore': 'nav.explore',
  '/reasoning': 'nav.reasoning',
  '/intentions': 'nav.intentions',
  '/hubs': 'nav.hubs',
  '/insights': 'nav.insights',
  '/decisions': 'nav.decisions',
  '/stats': 'nav.stats',
  '/settings': 'nav.settings',
  '/tutorial': 'nav.tutorial',
};

function resolveTitle(pathname: string): string | null {
  const clean = pathname.replace(/^\/dashboard/, '');
  if (ROUTE_TITLES[clean]) return ROUTE_TITLES[clean];
  for (const [route, key] of Object.entries(ROUTE_TITLES)) {
    if (route !== '/' && clean.startsWith(route)) return key;
  }
  return null;
}

export function RouteAnnouncer() {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const [announcement, setAnnouncement] = useState('');
  const prevPathRef = useRef(pathname);

  useEffect(() => {
    if (pathname === prevPathRef.current) return;
    prevPathRef.current = pathname;

    const titleKey = resolveTitle(pathname);
    if (!titleKey) return;

    const pageName = t(titleKey);

    // Updating `document.title` is the second half of a complete navigation
    // announcement — screen readers re-announce the title on route change,
    // browser tabs become identifiable, and history entries get real labels.
    // The aria-live region is for the in-page announcement; the title is
    // for everything outside the document.
    document.title = `${pageName} · ${t('app.name')}`;

    requestAnimationFrame(() => {
      setAnnouncement(t('a11y.navigatedTo', { page: pageName }));
    });
  }, [pathname, t]);

  return (
    <div role="status" aria-live="assertive" aria-atomic="true" className="sr-only">
      {announcement}
    </div>
  );
}
