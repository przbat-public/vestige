import { useTranslation } from 'react-i18next';
import { Link } from 'react-router';
import { EVENT, track } from '@/stores/telemetry';

interface PageCTAProps {
  /** Route path without leading slash, e.g. "graph" or "memories". */
  to: string;
  /** Stable key for telemetry; usually matches the Step's id (e.g. "briefing"). */
  pageKey: string;
}

/**
 * "Open this page →" link rendered on every Step card.
 *
 * The Step card describes a page; the CTA lets the user act on the
 * description without hunting for the page in the side nav. Telemetry
 * lets us see which page descriptions actually lead users to the page
 * (and which don't — maybe the description doesn't sell the page well).
 */
export function PageCTA({ to, pageKey }: PageCTAProps) {
  const { t } = useTranslation();
  return (
    <Link
      to={`/${to}`}
      onClick={() => track(EVENT.tutorial_page_cta, { page: pageKey })}
      className="inline-flex items-center gap-1 mt-3 text-xs font-medium text-primary hover:underline focus:outline-none focus:ring-2 focus:ring-ring rounded-sm"
    >
      {t('tutorial.cta.openPage')}
      <span aria-hidden="true">→</span>
    </Link>
  );
}
