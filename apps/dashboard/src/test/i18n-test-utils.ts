import { readdirSync, readFileSync, statSync } from 'node:fs';
import { extname, relative, resolve } from 'node:path';

/**
 * Shared helpers for the locale contract tests.
 *
 * The dashboard ships two locales (EN/PL) and the TypeScript compiler cannot
 * see inside JSON, so every guarantee here has to come from a test. These
 * helpers keep the three checks honest:
 *   • `src/test/i18n-locales.test.ts`  — locale-vs-locale parity, placeholders,
 *     CLDR plural forms;
 *   • `src/test/i18n-usage.test.ts`    — keys referenced from `.tsx`/`.ts`
 *     exist, and no user-facing copy is hardcoded.
 *
 * Lives under `src/test/` (not `src/lib/`) so no production module can
 * accidentally pull `node:fs` into the browser bundle.
 */

export type LocaleKeyMap = Record<string, string>;

/** Flatten nested locale JSON to `dot.separated.path` → string. */
export function flatten(value: unknown, prefix = '', out: LocaleKeyMap = {}): LocaleKeyMap {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
      flatten(child, prefix ? `${prefix}.${key}` : key, out);
    }
  } else if (typeof value === 'string') {
    out[prefix] = value;
  }
  return out;
}

export function loadLocale(locale: 'en' | 'pl'): LocaleKeyMap {
  const path = resolve(__dirname, `../i18n/${locale}.json`);
  return flatten(JSON.parse(readFileSync(path, 'utf8')));
}

/**
 * CLDR plural suffixes i18next must find for each locale.
 *
 * English only distinguishes one/other; Polish needs one/few/many/other —
 * without them i18next silently falls back to the base string and renders
 * "2 wspomnień" instead of "2 wspomnienia".
 */
export const PLURAL_SUFFIXES: Record<'en' | 'pl', string[]> = {
  en: ['_one', '_other'],
  pl: ['_one', '_few', '_many', '_other'],
};

const PLURAL_SUFFIX_PATTERN = /_(zero|one|two|few|many|other)$/;

/** Strip a trailing CLDR plural suffix, e.g. `status.memoriesCount_few`. */
export function pluralBase(key: string): string {
  return key.replace(PLURAL_SUFFIX_PATTERN, '');
}

/** True when the value interpolates `{{count}}` (i.e. drives pluralisation). */
export function usesCount(value: string): boolean {
  return value.includes('{{count}}');
}

/** Placeholder names (`{{name}}`) in order of first appearance. */
export function placeholders(value: string): string[] {
  return [...value.matchAll(/\{\{(\w+)\}\}/g)].map((match) => match[1]);
}

/** Recursively list source files under `dir`, skipping tests and specs. */
export function listSourceFiles(dir: string, exts: readonly string[] = ['.ts', '.tsx']): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = resolve(dir, entry);
    if (statSync(full).isDirectory()) {
      found.push(...listSourceFiles(full, exts));
    } else if (exts.includes(extname(entry)) && !/\.(test|spec)\.tsx?$/.test(entry)) {
      found.push(full);
    }
  }
  return found;
}

export function readSource(path: string): string {
  return readFileSync(path, 'utf8');
}

export function relativeToSrc(path: string): string {
  return relative(resolve(__dirname, '..'), path);
}

/** `src/` root, resolved from this helper's location. */
export const SRC_DIR = resolve(__dirname, '..');
