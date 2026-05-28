import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/lib/i18n';
import { LanguageSwitcher } from './LanguageSwitcher';

function renderSwitcher() {
  return render(
    <I18nextProvider i18n={i18n}>
      <LanguageSwitcher />
    </I18nextProvider>,
  );
}

describe('LanguageSwitcher', () => {
  beforeAll(async () => {
    await i18n.changeLanguage('en');
    await i18n.loadNamespaces(['translation']);
  });

  beforeEach(() => {
    document.documentElement.lang = 'en';
    localStorage.removeItem('vestige-language');
  });

  it('flips document.documentElement.lang when the user picks Polish', async () => {
    // Screen readers and CJK/PL/etc. fonts cascade off <html lang>.
    // If we only swap i18n bundles and leave the HTML attribute as
    // "en", VoiceOver narrates Polish content with an English engine
    // — broken pronunciation for every blind user. This test guards
    // against silent regressions where someone refactors the
    // switcher and forgets the DOM side effect.
    const user = userEvent.setup();
    renderSwitcher();
    await user.click(screen.getByRole('button', { name: /polski/i }));
    await waitFor(() => expect(document.documentElement.lang).toBe('pl'));
  });

  it('syncs document.documentElement.lang to the persisted i18n language', async () => {
    // Initial dashboard load with `localStorage["vestige-language"]
    // = "pl"` should hydrate <html lang="pl"> *before* the user
    // interacts. The old code only touched the attribute inside
    // the switcher's onClick handler, so a returning Polish user
    // saw <html lang="en"> until they re-picked their language.
    // The fix lives in lib/i18n.ts (an `on('languageChanged')`
    // listener), so we exercise it by changing the language
    // imperatively the same way the bootstrap does.
    document.documentElement.lang = 'en';
    await i18n.changeLanguage('pl');
    await waitFor(() => expect(document.documentElement.lang).toBe('pl'));
    await i18n.changeLanguage('en');
  });
});
