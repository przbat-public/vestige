import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { getLastSeenVersion, setLastSeenVersion } from '@/stores/tutorial-progress';
import { getNewEntriesSince } from './changelog';

interface WhatsNewBannerProps {
  /** Current dashboard version (from `package.json`, injected by the page). */
  currentVersion: string;
}

/**
 * Surfaces tutorial-relevant changes since the user's last visit.
 *
 * Acknowledgement model:
 *   - First-ever visit (`lastSeen === null`) — banner stays silent and we
 *     stamp the current version on render. The first read of the tutorial
 *     is enough; we don't want to point at things the user has never seen
 *     and call them "new".
 *   - Later visits — show every entry strictly newer than `lastSeen`.
 *     Dismissing the banner updates `lastSeen` to `currentVersion`, so
 *     reopening the tutorial later won't repeat the same entries.
 *
 * Why this and not a toast: a toast disappears. A banner attached to the
 * top of the tutorial sticks around until the user dismisses it, giving
 * them a chance to skim while they're already in tutorial mode.
 */
export function WhatsNewBanner({ currentVersion }: WhatsNewBannerProps) {
  const { t } = useTranslation();
  const [dismissed, setDismissed] = useState(false);

  const entries = useMemo(() => {
    const lastSeen = getLastSeenVersion();
    if (lastSeen === null) {
      // Stamp on first ever visit, render nothing.
      setLastSeenVersion(currentVersion);
      return [];
    }
    return getNewEntriesSince(lastSeen);
  }, [currentVersion]);

  if (dismissed || entries.length === 0) return null;

  const dismiss = () => {
    setLastSeenVersion(currentVersion);
    setDismissed(true);
  };

  return (
    <div className="flex items-start gap-3 p-3 rounded-lg border border-primary/20 bg-primary/5">
      <span aria-hidden="true" className="text-primary text-base leading-none mt-0.5">
        ✨
      </span>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-foreground">
          {t('tutorial.whatsNew.header', { count: entries.length })}
        </div>
        <ul className="mt-1.5 space-y-1.5">
          {entries.map((entry) => (
            <li key={entry.version} className="text-xs leading-relaxed">
              <span className="font-mono text-muted-foreground mr-2">v{entry.version}</span>
              <span className="font-medium text-foreground">{t(entry.titleKey)}</span>
              <span className="text-muted-foreground"> — {t(entry.bodyKey)}</span>
            </li>
          ))}
        </ul>
      </div>
      <button
        type="button"
        onClick={dismiss}
        className="text-xs text-muted-foreground hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring rounded-sm px-2 py-0.5"
        aria-label={t('tutorial.whatsNew.dismissAria')}
      >
        {t('tutorial.whatsNew.dismiss')}
      </button>
    </div>
  );
}
