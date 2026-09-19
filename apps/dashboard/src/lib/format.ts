/**
 * Locale-aware date/time formatting.
 *
 * Nine call sites used the argument-less `toLocaleString()` /
 * `toLocaleDateString()` / `toLocaleTimeString()`, which formats with
 * `navigator.language` — not the language the user picked in the dashboard.
 * A Polish user on an en-US browser saw `1/2/2026, 3:04:05 PM` inside a Polish
 * UI, often next to a correctly formatted date from a component that did pass
 * the locale.
 *
 * Every caller already has `useTranslation()`, so `i18n.language` is one
 * argument away. Normalising through this module also keeps the *style* in one
 * place (e.g. date-only vs date+time) instead of re-deciding per component.
 *
 * i18next is the source of truth for the locale, not `Intl`'s defaults: the
 * `pl` pack needs `pl-PL` (not bare `pl`) to pick Polish month names in every
 * engine, hence the explicit map.
 */

const INTL_LOCALES: Record<string, string> = {
  en: 'en-US',
  pl: 'pl-PL',
};

/** Map an i18next language tag (`pl`, `en-GB`, …) to an `Intl` locale. */
export function intlLocale(language: string | undefined): string {
  if (!language) return 'en-US';
  if (INTL_LOCALES[language]) return INTL_LOCALES[language];
  const base = language.split('-')[0];
  return INTL_LOCALES[base] ?? language;
}

type DateInput = string | number | Date | null | undefined;

function toDate(value: DateInput): Date | null {
  if (value === null || value === undefined || value === '') return null;
  const date = value instanceof Date ? value : new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** Date + time, e.g. `02.01.2026, 15:04:05`. Returns `''` for invalid input. */
export function formatDateTime(value: DateInput, language: string | undefined): string {
  const date = toDate(value);
  if (!date) return '';
  return date.toLocaleString(intlLocale(language));
}

/** Date only, e.g. `02.01.2026`. Returns `''` for invalid input. */
export function formatDate(value: DateInput, language: string | undefined): string {
  const date = toDate(value);
  if (!date) return '';
  return date.toLocaleDateString(intlLocale(language));
}

/** Time only, e.g. `15:04:05`. Returns `''` for invalid input. */
export function formatTime(value: DateInput, language: string | undefined): string {
  const date = toDate(value);
  if (!date) return '';
  return date.toLocaleTimeString(intlLocale(language));
}

/**
 * Assertion-friendly check used by the locale tests: no production module may
 * call the argument-less `toLocale*` variants directly.
 */
export const LOCALE_AWARE_HELPERS = ['formatDate', 'formatDateTime', 'formatTime'] as const;
