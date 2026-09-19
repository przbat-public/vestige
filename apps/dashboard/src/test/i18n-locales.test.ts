import { type LocaleKeyMap, loadLocale, PLURAL_SUFFIXES, placeholders, pluralBase, usesCount } from './i18n-test-utils';

/**
 * Locale contract: EN and PL must describe the same key space, interpolate the
 * same placeholders, and carry a complete CLDR plural set for every counted
 * string.
 *
 * These tests exist because i18next fails *silently*: a missing key renders
 * the key (or a hardcoded English `defaultValue`), and a missing plural form
 * falls back to the base string — which produced "2 wspomnień" in the Polish
 * UI with nothing in the test output to show for it.
 */

const LOCALES = ['en', 'pl'] as const;

const messages: Record<(typeof LOCALES)[number], LocaleKeyMap> = {
  en: loadLocale('en'),
  pl: loadLocale('pl'),
};

const en = messages.en;
const pl = messages.pl;

const SUFFIX_PATTERN = /_(zero|one|two|few|many|other)$/;

describe('locales — key parity', () => {
  it('every EN key exists in PL', () => {
    const missing = Object.keys(en)
      .filter((key) => !(key in pl))
      .sort();
    expect(missing).toEqual([]);
  });

  it('every PL key exists in EN, except the CLDR forms English does not use', () => {
    // `_few` / `_many` are required in Polish and meaningless in English;
    // i18next resolves them to `_other`, so their absence from en.json is
    // correct. Any *other* PL-only key is drift.
    const extra = Object.keys(pl)
      .filter((key) => !(key in en))
      .filter((key) => !/_(few|many)$/.test(key))
      .sort();
    expect(extra).toEqual([]);
  });

  it('has no empty strings', () => {
    for (const locale of LOCALES) {
      const empty = Object.entries(messages[locale])
        .filter(([, value]) => value.trim() === '')
        .map(([key]) => key);
      expect({ locale, empty }).toEqual({ locale, empty: [] });
    }
  });
});

describe('locales — interpolation placeholders', () => {
  it('uses the same placeholders in EN and PL', () => {
    const mismatches: Array<{ key: string; en: string[]; pl: string[] }> = [];
    for (const [key, enValue] of Object.entries(en)) {
      const plValue = pl[key];
      if (typeof plValue !== 'string') continue;
      const enSlots = [...placeholders(enValue)].sort();
      const plSlots = [...placeholders(plValue)].sort();
      if (enSlots.join('|') !== plSlots.join('|')) {
        mismatches.push({ key, en: enSlots, pl: plSlots });
      }
    }
    expect(mismatches).toEqual([]);
  });

  it('keeps plural variants of one key in sync with each other', () => {
    const mismatches: Array<{ key: string; slots: string[] }> = [];
    for (const [key, value] of Object.entries(en)) {
      const base = pluralBase(key);
      const siblings = [en[base], pl[base], ...PLURAL_SUFFIXES.en.map((s) => en[`${base}${s}`])].filter(
        (v): v is string => typeof v === 'string',
      );
      const expected = [...placeholders(value)].sort().join('|');
      for (const sibling of siblings) {
        if ([...placeholders(sibling)].sort().join('|') !== expected) {
          mismatches.push({ key, slots: placeholders(sibling) });
        }
      }
    }
    expect(mismatches).toEqual([]);
  });
});

describe('locales — plural forms', () => {
  it('every counted key carries a full CLDR set in each locale', () => {
    const incomplete: Array<{ locale: string; key: string; missing: string[] }> = [];
    for (const locale of LOCALES) {
      const catalog = messages[locale];
      const countedBases = new Set(
        Object.entries(catalog)
          .filter(([key, value]) => usesCount(value) && !SUFFIX_PATTERN.test(key))
          .map(([key]) => key),
      );
      for (const base of countedBases) {
        const missing = PLURAL_SUFFIXES[locale].filter((suffix) => !(`${base}${suffix}` in catalog));
        if (missing.length > 0) incomplete.push({ locale, key: base, missing });
      }
    }
    expect(incomplete).toEqual([]);
  });

  it('never ships a *partial* plural set for a base key', () => {
    // The catalog uses two conventions side by side: keys that only ever get a
    // `count` carry just the CLDR forms (`nav.reviewDueAria_one` / `_other`,
    // no base), while keys that double as uncounted labels keep the base too.
    // Either is fine — but a base with *some* forms missing makes i18next fall
    // back to the base string for the missing category, which is exactly how
    // "2 wspomnień" got into the Polish UI.
    const partial: Array<{ locale: string; key: string; missing: string[] }> = [];
    for (const locale of LOCALES) {
      const catalog = messages[locale];
      const bases = new Set(
        Object.keys(catalog)
          .filter((key) => SUFFIX_PATTERN.test(key))
          .map((key) => pluralBase(key)),
      );
      for (const base of bases) {
        const missing = PLURAL_SUFFIXES[locale].filter((suffix) => !(`${base}${suffix}` in catalog));
        if (missing.length > 0) partial.push({ locale, key: base, missing });
      }
    }
    expect(partial).toEqual([]);
  });

  it('translates a counted string with real Polish grammar', () => {
    // Spot-check the categories i18next picks for pl: 1 → one, 2/3/4 → few,
    // 5+ → many. This is the user-visible symptom the audit found in the
    // sidebar memory counter.
    expect(pl['status.memoriesCount_one']).toBe('{{count}} wspomnienie');
    expect(pl['status.memoriesCount_few']).toBe('{{count}} wspomnienia');
    expect(pl['status.memoriesCount_many']).toBe('{{count}} wspomnień');
    expect(en['status.memoriesCount_one']).toBe('{{count}} memory');
    expect(en['status.memoriesCount_other']).toBe('{{count}} memories');
  });
});
