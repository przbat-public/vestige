import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import type { MemoryRevisions } from '@/types';
import { MemoryRevisionsPanel } from './MemoryRevisionsPanel';

const memoryId = '11111111-1111-4111-9111-111111111111';

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <MemoryRevisionsPanel memoryId={memoryId} />
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('MemoryRevisionsPanel', () => {
  beforeAll(async () => {
    await i18n.changeLanguage('en');
    await i18n.loadNamespaces(['translation']);
  });

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('lazy-loads — does not fetch on initial render (closed by default)', async () => {
    const spy = vi.spyOn(api.memories, 'revisions');
    renderPanel();
    await new Promise((r) => setTimeout(r, 50));
    expect(spy).not.toHaveBeenCalled();
  });

  it('fetches and renders content revisions when the user opens the disclosure', async () => {
    const fixture: MemoryRevisions = {
      memoryId,
      total: 2,
      revisions: [
        {
          id: 2,
          kind: 'edit',
          recordedAt: '2026-05-08T10:00:00Z',
          oldContent: 'genesis wording',
          newContent: 'second wording',
          reason: 'clarified the antecedent',
          actor: 'dashboard',
        },
        {
          id: 1,
          kind: 'create',
          recordedAt: '2026-05-07T10:00:00Z',
          oldContent: undefined,
          newContent: 'genesis wording',
          reason: undefined,
          actor: undefined,
        },
      ],
    };
    vi.spyOn(api.memories, 'revisions').mockResolvedValue(fixture);

    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByText(/content history/i));

    // The edit's "before" and the create's "after" carry the same text, so the
    // pair is the assertion that both halves are rendered.
    expect(await screen.findAllByText(/genesis wording/)).toHaveLength(2);
    expect(screen.getByText(/second wording/)).toBeInTheDocument();
    expect(screen.getByText(/clarified the antecedent/)).toBeInTheDocument();
    expect(screen.getByText(/by dashboard/)).toBeInTheDocument();
    expect(screen.getByText(/edited/i)).toBeInTheDocument();
    // The label must not claim to be the state-transition changelog.
    expect(screen.queryByText(/edit history/i)).not.toBeInTheDocument();
  });

  it('shows the empty state when the memory has no revisions', async () => {
    vi.spyOn(api.memories, 'revisions').mockResolvedValue({ memoryId, total: 0, revisions: [] });

    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByText(/content history/i));
    expect(await screen.findByText(/no content revisions recorded yet/i)).toBeInTheDocument();
  });

  it('shows the error panel with retry when the request fails', async () => {
    vi.spyOn(api.memories, 'revisions').mockRejectedValue(new Error('Backend exploded'));

    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByText(/content history/i));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /try again/i })).toBeInTheDocument();
  });
});
