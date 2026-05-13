import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { LOCALE_LABELS, SUPPORTED_LOCALES } from '@/lib/i18n';

export function LanguageSwitcher() {
  const { i18n } = useTranslation();
  const queryClient = useQueryClient();

  const changeLanguage = (lang: string) => {
    i18n.changeLanguage(lang);
    localStorage.setItem('vestige-language', lang);
    document.documentElement.lang = lang;
    queryClient.invalidateQueries();
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
