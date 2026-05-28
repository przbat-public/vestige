import { useTranslation } from 'react-i18next';
import { LOCALE_LABELS, SUPPORTED_LOCALES } from '@/lib/i18n';

export function LanguageSwitcher() {
  const { i18n } = useTranslation();

  // No `queryClient.invalidateQueries()` here on purpose — the backend
  // returns identifiers and timestamps, not localised strings, so a language
  // switch only needs to swap i18n bundles client-side. The previous
  // unconditional invalidation forced a full dashboard refetch (≥10 round
  // trips) on every language change.
  //
  // `document.documentElement.lang` is no longer set here — `lib/i18n.ts`
  // owns the side effect via an `on('languageChanged')` listener so it
  // also runs at initial hydration (when a returning user lands with a
  // persisted non-English locale). Keeping both producers would risk
  // drift if one path is updated and the other is forgotten.
  const changeLanguage = (lang: string) => {
    i18n.changeLanguage(lang);
    localStorage.setItem('vestige-language', lang);
  };

  return (
    <div className="flex gap-1">
      {SUPPORTED_LOCALES.map((locale) => (
        <button
          key={locale}
          type="button"
          onClick={() => changeLanguage(locale)}
          className={`px-2 py-1 rounded text-xs transition-colors ${
            i18n.language === locale
              ? 'bg-primary/15 text-primary font-medium'
              : 'text-muted-foreground hover:text-foreground hover:bg-accent'
          }`}
          aria-pressed={i18n.language === locale}
          aria-label={LOCALE_LABELS[locale]}
        >
          {locale.toUpperCase()}
        </button>
      ))}
    </div>
  );
}
