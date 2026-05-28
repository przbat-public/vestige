import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { useDialogStore } from '@/stores/dialogs';
import type { Insight } from '@/types';
import { InsightCard } from './InsightCard';

const mockNavigate = vi.fn();
vi.mock('react-router', async () => {
  const actual = (await vi.importActual<typeof import('react-router')>('react-router')) as Record<string, unknown>;
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

function makeInsight(overrides: Partial<Insight> = {}): Insight {
  return {
    id: 'insight-1',
    content: 'Three of your last five OAuth bugs were token-cache invalidation.',
    insightType: 'pattern',
    origin: 'dream',
    sourceMemoryIds: ['src-aaaaaaaa-1111', 'src-bbbbbbbb-2222'],
    confidence: 0.82,
    novelty: 0.34,
    validated: false,
    createdAt: '2026-05-01T12:00:00Z',
    tags: ['oauth'],
    ...overrides,
  } as Insight;
}

function renderCard(insight: Insight) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <InsightCard insight={insight} />
        </MemoryRouter>
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

describe('InsightCard source memory links', () => {
  beforeEach(() => {
    mockNavigate.mockReset();
    useDialogStore.setState({ pendingSelectMemoryId: null });
  });

  it('renders source memory ids as buttons, never as <a href="/memories/:id">', () => {
    renderCard(makeInsight());
    expect(screen.getByRole('button', { name: /src-aaaa/i })).toBeInTheDocument();
    const anchors = document.querySelectorAll('a[href^="/memories/"]');
    expect(anchors.length).toBe(0);
  });

  it('clicking a source id stages it in the dialog store and navigates to /memories', async () => {
    const user = userEvent.setup();
    renderCard(makeInsight());
    await user.click(screen.getByRole('button', { name: /src-bbbb/i }));
    expect(useDialogStore.getState().pendingSelectMemoryId).toBe('src-bbbbbbbb-2222');
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });

  it('clicking the insight content itself opens the insight memory drawer', async () => {
    // Insights are first-class memories — `id` is the host KnowledgeNode.
    // Without a click-through on the content the user could promote /
    // dismiss / drill into sources, but never inspect the insight node
    // itself (tags, retention, edit history, related memories,
    // contradictions). Mirrors HubCard, TemporalEntryCard, and DecisionCard.
    const user = userEvent.setup();
    renderCard(makeInsight());

    const trigger = screen.getByRole('button', { name: /Three of your last five OAuth bugs/i });
    await user.click(trigger);

    expect(useDialogStore.getState().pendingSelectMemoryId).toBe('insight-1');
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });
});
