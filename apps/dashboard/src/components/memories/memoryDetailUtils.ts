/** Parse the comma/newline separated tags input into a clean array. */
export function parseTagsInput(value: string): string[] {
  return value
    .split(/[,\n]/)
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
}

/**
 * Human-friendly relative time-until string for FSRS-6 next-review timestamps.
 *
 * - Past dates → `dueNowLabel` (the FSRS scheduler treats ≤ now as eligible)
 * - <1 hour    → "in 12 minutes"
 * - <1 day     → "in 5 hours"
 * - ≥1 day     → "in 3 days"
 *
 * Returns null when the input is missing/invalid so callers can skip the badge.
 * Uses Intl.RelativeTimeFormat for proper PL/EN inflection ("za 5 godzin").
 */
export function formatNextReview(
  iso: string | undefined | null,
  locale: string,
  dueNowLabel: string,
): { label: string; isOverdue: boolean } | null {
  if (!iso) return null;
  const target = Date.parse(iso);
  if (Number.isNaN(target)) return null;
  const deltaMs = target - Date.now();
  if (deltaMs <= 0) return { label: dueNowLabel, isOverdue: true };

  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  const minutes = Math.round(deltaMs / 60_000);
  if (minutes < 60) return { label: rtf.format(minutes, 'minute'), isOverdue: false };
  const hours = Math.round(deltaMs / 3_600_000);
  if (hours < 24) return { label: rtf.format(hours, 'hour'), isOverdue: false };
  const days = Math.round(deltaMs / 86_400_000);
  return { label: rtf.format(days, 'day'), isOverdue: false };
}
