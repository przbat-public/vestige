import i18n from 'i18next';
import resourcesToBackend from 'i18next-resources-to-backend';
import { initReactI18next } from 'react-i18next';

const LOCALE_LOADERS: Record<string, () => Promise<{ default: Record<string, unknown> }>> = {
  en: () => import('@/i18n/en.json'),
  pl: () => import('@/i18n/pl.json'),
};

export const SUPPORTED_LOCALES = Object.keys(LOCALE_LOADERS);

export const LOCALE_LABELS: Record<string, string> = {
  en: 'English',
  pl: 'Polski',
};

const savedLang = typeof window !== 'undefined' ? localStorage.getItem('vestige-language') : null;

i18n
  .use(
    resourcesToBackend((language: string) => {
      const loader = LOCALE_LOADERS[language];
      if (!loader) return Promise.reject(new Error(`Unknown locale: ${language}`));
      return loader();
    }),
  )
  .use(initReactI18next)
  .init({
    lng: savedLang || 'en',
    fallbackLng: 'en',
    interpolation: { escapeValue: false },
  });

// Keep `<html lang>` in sync with the active locale. Previously this
// was only set inside `LanguageSwitcher` on click — meaning a
// returning user with `localStorage["vestige-language"]="pl"`
// hydrated with `<html lang="en">` until they re-picked their
// language. The attribute drives screen-reader pronunciation,
// CSS `:lang()` selectors, and the browser's hyphenation rules,
// so the gap was a real accessibility regression.
if (typeof document !== 'undefined') {
  const syncHtmlLang = (lng: string) => {
    document.documentElement.lang = lng;
  };
  syncHtmlLang(i18n.language);
  i18n.on('languageChanged', syncHtmlLang);
}

function preloadFallback() {
  const current = i18n.language.slice(0, 2);
  if (current !== 'en') {
    i18n.loadLanguages('en');
  }
}

if (typeof window !== 'undefined') {
  if ('requestIdleCallback' in window) {
    window.requestIdleCallback(preloadFallback);
  } else {
    setTimeout(preloadFallback, 1000);
  }
}

export default i18n;
