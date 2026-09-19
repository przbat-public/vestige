import { formatDate, formatDateTime, formatTime, intlLocale } from './format';

describe('intlLocale', () => {
  it('maps the i18next language tag to a region-qualified Intl locale', () => {
    expect(intlLocale('pl')).toBe('pl-PL');
    expect(intlLocale('en')).toBe('en-US');
  });

  it('honours an explicit region and falls back for unknown tags', () => {
    expect(intlLocale('pl-GB')).toBe('pl-PL');
    expect(intlLocale('de')).toBe('de');
    expect(intlLocale(undefined)).toBe('en-US');
  });
});

describe('format helpers', () => {
  // A date whose day/month order differs between the two locales, so a
  // missing locale argument cannot accidentally pass.
  const sample = '2026-01-02T15:04:05Z';

  it('formats with the language it is given, not the browser default', () => {
    const pl = formatDateTime(sample, 'pl');
    const en = formatDateTime(sample, 'en');
    expect(pl).not.toBe('');
    expect(en).not.toBe('');
    expect(pl).not.toBe(en);
  });

  it('formats date and time independently', () => {
    expect(formatDate(sample, 'pl')).not.toContain(':');
    expect(formatTime(sample, 'pl')).toMatch(/\d{2}:\d{2}/);
  });

  it('accepts Date instances and timestamps', () => {
    const date = new Date(sample);
    expect(formatDate(date, 'en')).toBe(formatDate(sample, 'en'));
    expect(formatDate(date.getTime(), 'en')).toBe(formatDate(sample, 'en'));
  });

  it('returns an empty string for missing or invalid input', () => {
    for (const bad of [null, undefined, '', 'not-a-date']) {
      expect(formatDate(bad, 'pl')).toBe('');
      expect(formatDateTime(bad, 'pl')).toBe('');
      expect(formatTime(bad, 'pl')).toBe('');
    }
  });
});
