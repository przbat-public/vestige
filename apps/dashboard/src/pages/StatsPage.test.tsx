import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { useDialogStore } from '@/stores/dialogs';
import { StatsPage } from './StatsPage';

const mockNavigate = vi.fn();
vi.mock('react-router', async () => {
  const actual = (await vi.importActual<typeof import('react-router')>('react-router')) as Record<string, unknown>;
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

vi.mock('@/stores/api', () => ({
  api: {
    stats: () => Promise.resolve(makeStats()),
    health: () => Promise.resolve(makeHealth()),
    retentionDistribution: () => Promise.resolve(makeDistribution()),
  },
}));

function makeStats() {
  return {
    totalMemories: 1234,
    dueForReview: 12,
    averageRetention: 0.72,
    averageStorageStrength: 0.55,
    embeddingCoverage: 98,
  };
}

function makeHealth() {
  return { status: 'healthy', version: '3.4.0' };
}

function makeDistribution() {
  return {
    distribution: [
      { range: '0.0-0.1', count: 2 },
      { range: '0.9-1.0', count: 5 },
    ],
    byType: { fact: 10, note: 5 },
    endangered: [
      {
        id: 'endangered-mem-aaa',
        content: 'Fading memory about OAuth tokens',
        retention: 0.18,
        nodeType: 'fact',
      },
      {
        id: 'endangered-mem-bbb',
        content: 'Another endangered note',
        retention: 0.25,
        nodeType: 'note',
      },
    ],
    total: 1234,
  };
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <StatsPage />
        </MemoryRouter>
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

describe('StatsPage', () => {
  beforeAll(async () => {
    await i18n.changeLanguage('en');
    await i18n.loadNamespaces(['translation']);
  });

  beforeEach(() => {
    mockNavigate.mockReset();
    useDialogStore.setState({ pendingSelectMemoryId: null });
  });

  it('renders endangered memories as clickable buttons that open the host memory drawer', async () => {
    // Before this fix, endangered memories rendered as flat <div>s with
    // no affordance — the user could see which memories were fading but
    // had no way to act on them ("which one is that? what does it say
    // in full? when did I last review it?"). Surfacing them in a panel
    // that you can't drill into is dashboard cosplay, not a tool. The
    // contract: each row is a button that stages the id and routes to
    // /memories so MemoriesPage can open the drawer.
    const user = userEvent.setup();
    renderPage();

    const trigger = await screen.findByRole('button', { name: /Fading memory about OAuth/i });
    await user.click(trigger);

    expect(useDialogStore.getState().pendingSelectMemoryId).toBe('endangered-mem-aaa');
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });

  it('localizes the system health status label instead of leaking the raw enum value', async () => {
    // Bare `<span className="capitalize">{health.status}</span>` shipped
    // "healthy" / "degraded" / "critical" / "empty" verbatim. A Polish
    // user saw "Healthy", which is both untranslated and exposes the
    // server-side enum. The contract: each status renders through the
    // `stats.health.*` namespace so language switches actually do
    // anything.
    renderPage();
    await waitFor(() => expect(screen.queryByText(i18n.t('stats.health.healthy'))).toBeInTheDocument());
    // Regression guard: the raw enum value must not appear in the DOM.
    // (`title="..."` attributes are fine; this query looks at text nodes.)
    expect(screen.queryByText('healthy')).toBeNull();
  });
});
