import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter, Route, Routes, useNavigate } from 'react-router';
import { NAV_ITEMS_FLAT } from '@/components/layout/nav-sections';
import i18n from '@/lib/i18n';
import { ROUTE_TITLE_KEYS, RouteAnnouncer } from './RouteAnnouncer';

/**
 * Route-change feedback for keyboard and screen-reader users.
 *
 * Two defects motivated these tests: `/temporal` existed in the sidebar but
 * had no entry in the announcer's title table (so navigating there produced
 * no announcement and left `document.title` on the previous page), and the
 * announcer never moved focus, so the next Tab continued through the sidebar
 * instead of the page the user had just opened (WCAG 2.4.3).
 */

function Harness({ initial = '/memories' }: { initial?: string }) {
  return (
    <I18nextProvider i18n={i18n}>
      <MemoryRouter initialEntries={[initial]}>
        <RouteAnnouncer />
        <main id="main-content" tabIndex={-1}>
          <Routes>
            <Route path="*" element={<p>page</p>} />
          </Routes>
        </main>
        <Nav />
      </MemoryRouter>
    </I18nextProvider>
  );
}

function Nav() {
  const navigate = useNavigate();
  return (
    <button type="button" onClick={() => navigate('/temporal')}>
      go to temporal
    </button>
  );
}

describe('RouteAnnouncer', () => {
  beforeEach(() => {
    document.title = 'vestige';
    i18n.changeLanguage('en');
  });

  it('has a title key for every sidebar route', () => {
    // The announcer derives its table from NAV_SECTIONS, so this is the
    // invariant that used to be maintained by hand (and silently rotted —
    // `/temporal` shipped in the sidebar with no entry here).
    const missing = NAV_ITEMS_FLAT.filter((item) => !(`/${item.to}` in ROUTE_TITLE_KEYS));
    expect(missing).toEqual([]);
  });

  it('announces /temporal and updates the document title', async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(screen.getByRole('button', { name: 'go to temporal' }));

    await waitFor(() => {
      expect(document.title).toContain(i18n.t('nav.temporal'));
    });
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(i18n.t('nav.temporal'));
    });
  });

  it('moves focus to the main landmark on navigation', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const trigger = screen.getByRole('button', { name: 'go to temporal' });
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    await user.click(trigger);

    expect(document.activeElement).toBe(document.getElementById('main-content'));
  });
});
