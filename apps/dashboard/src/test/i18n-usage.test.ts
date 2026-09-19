import { listSourceFiles, loadLocale, PLURAL_SUFFIXES, readSource, relativeToSrc, SRC_DIR } from './i18n-test-utils';

/**
 * Usage contract: every translation key the source references must exist in
 * both locales, and user-facing copy must not be hardcoded.
 *
 * The audit found seven keys (`settings.doubtModeTooltip`,
 * `insights.confidenceTooltip`, …) that no locale defined. The call sites
 * passed an English `defaultValue`, so i18next rendered English text inside
 * the Polish UI and nothing failed — no type error, no runtime warning, no
 * test. These tests are the missing feedback loop.
 */

const en = loadLocale('en');
const pl = loadLocale('pl');

const SOURCE_FILES = listSourceFiles(SRC_DIR).filter((file) => !file.includes('/types/generated/'));

/** `t('a.b')` / `t("a.b")` with a *literal* key. */
const STATIC_KEY = /\bt\(\s*['"]([A-Za-z0-9_.]+)['"]/g;
/** Keys that are a static prefix plus an interpolation, e.g. `` `nodeTypes.${type}` ``. */
const DYNAMIC_KEY = /\bt\(\s*`([A-Za-z0-9_.]+)\.\$\{/g;

function collectKeys(regex: RegExp) {
  const found = new Map<string, string[]>();
  for (const file of SOURCE_FILES) {
    for (const match of readSource(file).matchAll(regex)) {
      const list = found.get(match[1]) ?? [];
      list.push(relativeToSrc(file));
      found.set(match[1], list);
    }
  }
  return found;
}

/**
 * A static `t('x')` call is satisfied either by the literal key or by a full
 * CLDR plural family (`x_one`, `x_other`, …) — i18next rewrites the lookup to
 * the suffixed form whenever the call passes `count`, and many keys in this
 * catalog exist *only* in plural form.
 */
function resolvesIn(catalog: Record<string, string>, key: string): boolean {
  if (key in catalog) return true;
  return PLURAL_SUFFIXES.en.every((suffix) => `${key}${suffix}` in catalog);
}

describe('i18n usage — every referenced key exists', () => {
  it('static t() keys resolve in EN and PL', () => {
    const missing: Array<{ key: string; missingIn: string[]; files: string[] }> = [];
    for (const [key, files] of collectKeys(STATIC_KEY)) {
      const missingIn = [...(resolvesIn(en, key) ? [] : ['en']), ...(resolvesIn(pl, key) ? [] : ['pl'])];
      if (missingIn.length > 0) missing.push({ key, missingIn, files: [...new Set(files)] });
    }
    expect(missing).toEqual([]);
  });

  it('members of dynamically built key families exist', () => {
    // `` t(`nodeTypes.${type}`) `` — the prefix is static, the leaf is not.
    // At least one concrete key under the prefix must exist, otherwise the
    // whole family silently renders raw keys.
    const missing: Array<{ prefix: string; files: string[] }> = [];
    for (const [prefix, files] of collectKeys(DYNAMIC_KEY)) {
      const has = Object.keys(en).some((key) => key.startsWith(`${prefix}.`));
      if (!has) missing.push({ prefix, files: [...new Set(files)] });
    }
    expect(missing).toEqual([]);
  });

  it('the seven keys the audit found missing are now defined', () => {
    // Named explicitly so a future refactor that drops them fails loudly
    // instead of silently regressing to English-only tooltips.
    const audited = [
      'settings.doubtModeTooltip',
      'settings.doubtModeTooltipAria',
      'insights.confidenceTooltip',
      'insights.noveltyTooltip',
      'memories.reviewsTooltip',
      'memories.temporalPanel.titleTooltip',
      'reasoning.intentTooltip',
    ];
    const missing = audited.filter((key) => !(key in en) || !(key in pl));
    expect(missing).toEqual([]);
  });

  it('no call site smuggles English copy through a literal defaultValue', () => {
    // `defaultValue` is legitimate only when the fallback is derived from data
    // (a raw enum value, a Zod message key). A *literal English sentence* as
    // the fallback is how the audit's bug survived in a green test suite.
    const offenders: Array<{ file: string; line: number; text: string }> = [];
    for (const file of SOURCE_FILES) {
      readSource(file)
        .split('\n')
        .forEach((line, index) => {
          if (isComment(line) || !/defaultValue\s*:/.test(line)) return;
          const literal = line.match(/defaultValue\s*:\s*['"`]([^'"`]{4,})['"`]/);
          if (literal) offenders.push({ file: relativeToSrc(file), line: index + 1, text: literal[1] });
        });
    }
    expect(offenders).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Hardcoded-copy detection
// ---------------------------------------------------------------------------

/** Inside a `//` line or a `/* … *\/` block (the biome-ignore directives). */
function isComment(line: string): boolean {
  const trimmed = line.trim();
  return trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*');
}

/** Strip `// …` and `/* … *\/` tails so inline comments aren't scanned as copy. */
function stripInlineComments(line: string): string {
  return line.replace(/\/\/.*$/, '').replace(/\/\*.*?\*\//g, '');
}

/** Regions that are never rendered: comments, type positions, CSS class lists. */
function isCodey(text: string): boolean {
  // `=`, `(`, `{`, `}`, `|`, `&`, `;`, `?` and `:` only appear in copy that
  // would look wrong anyway; in practice they mean a type annotation, an arrow
  // body or a JSX attribute that happened to sit between two angle brackets.
  return /[=(){}|&;?<>]|\bnew\s+Map\b/.test(text);
}

const ALLOWED_LITERALS = new Set([
  // Product / technology names: identical in every locale, translating them
  // would be wrong rather than merely unnecessary.
  'FSRS',
  'FSRS-6',
  'Jina Reranker v2 Base',
  'MCP',
  'Vestige',
  'JSON',
  'RRF',
  'BM25',
  'HNSW',
  'API',
  'REST',
  'WebSocket',
  'Three.js',
  'React',
  'SQLite',
  'ONNX',
  'PL',
  'EN',
  'OK',
]);

function looksLikeCopy(text: string): boolean {
  const trimmed = text.trim();
  if (trimmed.length < 4) return false;
  if (ALLOWED_LITERALS.has(trimmed)) return false;
  if (isCodey(trimmed)) return false;
  if (/\bTODO\b|\bFIXME\b|console\./.test(trimmed)) return false;
  // Two or more letter-words — catches "About doubt mode" while ignoring
  // "★", "2/3", "nomic-embed-text-v1.5" and "1,234".
  const words = trimmed.split(/\s+/).filter((word) => /[A-Za-zĄĆĘŁŃÓŚŹŻąćęłńóśźż]{2,}/.test(word));
  if (words.length < 2) return false;
  // Copy is letters, spaces and ordinary punctuation. Anything denser is code.
  const copyish = [...trimmed].filter((c) => /[\p{L}\p{N}\s.,!?…—–'’"%/:+×→-]/u.test(c)).length;
  return copyish === trimmed.length;
}

/** Text nodes between `>` and `<` on one line, skipping comments. */
function jsxTextNodes(source: string) {
  const nodes: Array<{ line: number; text: string }> = [];
  source.split('\n').forEach((line, index) => {
    if (isComment(line)) return;
    // `{/* … */}` and `{/* biome-ignore … */}` directives wrap JSX in prose
    // that the angle-bracket scan would otherwise read as copy.
    const scannable = stripInlineComments(line).replace(/\{\/\*.*?\*\/\}/g, '');
    // Type-level conditionals (`Z extends Dto ? true : never`) live between
    // generic brackets and mimic JSX text; a line carrying a ternary or an
    // `extends` clause is never a text node.
    if (/\bextends\b|\btype\s+\w+\s*=|\?\s*\w+\s*:|=>/.test(scannable)) return;
    for (const match of scannable.matchAll(/>([^<>{}]+)</g)) {
      nodes.push({ line: index + 1, text: match[1] });
    }
  });
  return nodes;
}

describe('i18n usage — no hardcoded user-facing copy', () => {
  it('JSX text nodes are translated', () => {
    const offenders: Array<{ file: string; line: number; text: string }> = [];
    for (const file of SOURCE_FILES) {
      for (const node of jsxTextNodes(readSource(file))) {
        if (looksLikeCopy(node.text)) {
          offenders.push({ file: relativeToSrc(file), line: node.line, text: node.text.trim() });
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it('literal aria-label / title / placeholder / alt props use the translator', () => {
    const PROPS = /\b(aria-label|aria-description|title|placeholder|alt)\s*=\s*"([^"]{4,})"/g;
    const offenders: Array<{ file: string; line: number; text: string }> = [];
    for (const file of SOURCE_FILES) {
      readSource(file)
        .split('\n')
        .forEach((line, index) => {
          if (isComment(line)) return;
          for (const match of line.matchAll(PROPS)) {
            if (looksLikeCopy(match[2])) {
              offenders.push({ file: relativeToSrc(file), line: index + 1, text: match[2] });
            }
          }
        });
    }
    expect(offenders).toEqual([]);
  });

  it('formats dates through the locale helper, never with the browser default', () => {
    // `new Date(x).toLocaleString()` uses `navigator.language`, so a Polish
    // user on an en-US browser saw US-formatted dates inside the Polish UI.
    // `src/lib/format.ts` is the only module allowed to call the `toLocale*`
    // family — everything else goes through `formatDate` / `formatDateTime`.
    const offenders: Array<{ file: string; line: number }> = [];
    for (const file of SOURCE_FILES) {
      if (relativeToSrc(file) === 'lib/format.ts') continue;
      readSource(file)
        .split('\n')
        .forEach((line, index) => {
          if (isComment(line)) return;
          // Argument-less call: `toLocaleString()` with nothing inside.
          if (/toLocale(String|DateString|TimeString)\(\s*\)/.test(line)) {
            offenders.push({ file: relativeToSrc(file), line: index + 1 });
          }
        });
    }
    expect(offenders).toEqual([]);
  });
});
