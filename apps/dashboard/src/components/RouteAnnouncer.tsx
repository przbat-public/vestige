import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation } from 'react-router';
import { NAV_ITEMS_FLAT } from './layout/nav-sections';

/**
 * Route → i18n title key, derived from the sidebar's information architecture
 * so the two cannot drift.
 *
 * The previous hand-maintained copy of this table silently fell behind: `/temporal`
 * shipped in `NavSections` but never here, so that page produced no announcement
 * and left `document.title` on the *previous* page's value (WCAG 2.4.2 / 2.4.8).
 */
const ROUTE_TITLES: Record<string, string> = Object.fromEntries(
  // `nav.*` keys already carry the "…" prefix; the sidebar links are relative
  // segments, hence the leading slash.
  NAV_ITEMS_FLAT.map((item) => [`/${item.to}`, item.labelKey]),
);

function resolveTitle(pathname: string): string | null {
  const clean = pathname.replace(/^\/dashboard/, '');
  if (ROUTE_TITLES[clean]) return ROUTE_TITLES[clean];
  // Longest prefix wins so `/memories/123` resolves to the memories page
  // rather than to whichever shorter route happens to be enumerated first.
  const match = Object.keys(ROUTE_TITLES)
    .filter((route) => route !== '/' && clean.startsWith(route))
    .sort((a, b) => b.length - a.length)[0];
  return match ? ROUTE_TITLES[match] : null;
}

/**
 * Move focus to the `<main>` landmark after a client-side navigation.
 *
 * Without this, keyboard and screen-reader users stay parked on the sidebar
 * link they just activated: the next Tab continues through the navigation
 * instead of the page they asked for (WCAG 2.4.3, Focus Order). Skipped on
 * first render, and skipped while a modal owns focus.
 */
function useRouteFocus(pathname: string) {
  const isFirstRender = useRef(true);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the focus move is keyed on the route — `pathname` is the trigger, not an input
  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    // A dialog (native or custom) owns focus while it is open; stealing it
    // would break the dialog's own trap.
    if (document.querySelector('[aria-modal="true"], dialog[open]')) return;
    const main = document.getElementById('main-content');
    if (!main) return;
    // After commit, before paint — the new page's DOM is in place and focus
    // does not visibly jump through the old content.
    main.focus({ preventScroll: true });
  }, [pathname]);
}

export function RouteAnnouncer() {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const [announcement, setAnnouncement] = useState('');
  const prevPathRef = useRef(pathname);

  useRouteFocus(pathname);

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

/** Exposed for the nav/title parity test. */
export const ROUTE_TITLE_KEYS = ROUTE_TITLES;
