import { useTranslation } from 'react-i18next';
import { useDensity } from '@/hooks/use-density';
import { EVENT, track } from '@/stores/telemetry';

/**
 * Density toggle button. Sits next to the theme toggle in the sidebar
 * footer; one click flips the dashboard between `comfortable` (default,
 * generous padding) and `compact` (denser cards for power users scanning
 * long memory feeds).
 *
 * The icon visualises the current state — three tight rows for compact,
 * two looser rows for comfortable. We label it via aria-label/title so
 * the tooltip reads "switch to compact" while currently comfortable, and
 * vice versa.
 */
export function DensityToggle() {
  const { isCompact, toggle } = useDensity();
  const { t } = useTranslation();

  const switchTo = isCompact
    ? t('common.densityComfortable', 'Comfortable density')
    : t('common.densityCompact', 'Compact density');

  const handleClick = () => {
    // Track the *target* density (where we're going) rather than the current
    // one — that's the analytically interesting bit.
    track(EVENT.density_toggle, { target: isCompact ? 'comfortable' : 'compact' });
    toggle();
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
      aria-label={switchTo}
      title={switchTo}
    >
      {isCompact ? (
        // Three tight rows representing compact spacing.
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          aria-hidden="true"
        >
          <line x1="4" y1="7" x2="20" y2="7" />
          <line x1="4" y1="12" x2="20" y2="12" />
          <line x1="4" y1="17" x2="20" y2="17" />
        </svg>
      ) : (
        // Two looser rows representing comfortable spacing.
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          aria-hidden="true"
        >
          <line x1="4" y1="8" x2="20" y2="8" />
          <line x1="4" y1="16" x2="20" y2="16" />
        </svg>
      )}
    </button>
  );
}
