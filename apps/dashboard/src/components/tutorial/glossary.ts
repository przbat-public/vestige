/**
 * Glossary — the single source of truth for jargon used across the tutorial.
 *
 * Why a separate module: the same definitions feed
 *   (a) the standalone Glossary card at the bottom of the tutorial, and
 *   (b) inline tooltips that turn words like "FSRS" into hover-revealable
 *       definitions everywhere they appear in body copy.
 *
 * Keeping the keys here (not inline at call sites) means we update one place
 * when terminology changes, and translators have a single bucket of strings
 * to scan instead of hunting them across the tutorial body.
 */

export const GLOSSARY_KEYS = [
  'retention',
  'storage',
  'retrieval',
  'fsrs',
  'embedding',
  'dream',
  'activation',
  'consolidation',
  'nodeType',
  'mcp',
] as const;

export type GlossaryKey = (typeof GLOSSARY_KEYS)[number];

/**
 * i18n lookups for one term:
 *   - `tutorial.glossary.terms.<key>` — the displayed name (short, e.g. "FSRS")
 *   - `tutorial.glossary.<key>`       — the full definition (a sentence or two)
 */
export function termLabelKey(key: GlossaryKey): string {
  return `tutorial.glossary.terms.${key}`;
}

export function termDefKey(key: GlossaryKey): string {
  return `tutorial.glossary.${key}`;
}
