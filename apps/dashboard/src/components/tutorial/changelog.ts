/**
 * Dashboard changelog — the single source of truth for the "What's new"
 * banner. Each entry is a tutorial-relevant change (a new page, a new
 * tool, a feature worth highlighting). Bugfixes do not belong here.
 *
 * The banner uses semver ordering, comparing the user's last-seen version
 * against the entries: anything strictly newer surfaces in the banner.
 *
 * Add entries to the top. Newest first.
 */

export interface ChangelogEntry {
  version: string;
  /** i18n key for the change summary, e.g. "tutorial.whatsNew.v3_3.title". */
  titleKey: string;
  /** i18n key for the longer body — keep to one sentence. */
  bodyKey: string;
}

export const CHANGELOG: readonly ChangelogEntry[] = [
  {
    version: '3.3.0',
    titleKey: 'tutorial.whatsNew.v3_3.title',
    bodyKey: 'tutorial.whatsNew.v3_3.body',
  },
  {
    version: '3.1.0',
    titleKey: 'tutorial.whatsNew.v3_1.title',
    bodyKey: 'tutorial.whatsNew.v3_1.body',
  },
];

/**
 * Compare two semver strings (a, b). Returns:
 *   > 0  if a > b
 *   < 0  if a < b
 *   = 0  if equal
 * Tolerates "3" / "3.1" / "3.1.0" — missing parts count as 0.
 */
function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map((s) => Number.parseInt(s, 10) || 0);
  const pb = b.split('.').map((s) => Number.parseInt(s, 10) || 0);
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i++) {
    const ai = pa[i] ?? 0;
    const bi = pb[i] ?? 0;
    if (ai !== bi) return ai - bi;
  }
  return 0;
}

/**
 * Pick the changelog entries that are strictly newer than `lastSeen`.
 * When `lastSeen` is null (first visit ever), return an empty array — we
 * don't want to scream "new!" at users who haven't even read the tutorial
 * once.
 */
export function getNewEntriesSince(lastSeen: string | null): readonly ChangelogEntry[] {
  if (!lastSeen) return [];
  return CHANGELOG.filter((e) => compareVersions(e.version, lastSeen) > 0);
}
