import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import type { MemoryChangelog } from '@/types';
import { MemoryChangelogPanel } from './MemoryChangelogPanel';

const memoryId = '11111111-1111-4111-9111-111111111111';

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <MemoryChangelogPanel memoryId={memoryId} />
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('MemoryChangelogPanel', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('lazy-loads — does not fetch on initial render (closed by default)', async () => {
    const spy = vi.spyOn(api.memories, 'changelog');
    renderPanel();
    // The disclosure starts closed; query is `enabled: opened === true`
    // so spy should not be called at all.
    await new Promise((r) => setTimeout(r, 50));
    expect(spy).not.toHaveBeenCalled();
  });

  it('fetches and renders transitions when the user opens the disclosure', async () => {
    const fixture: MemoryChangelog = {
      memoryId,
      memoryContent: 'Test',
      totalTransitions: 2,
      transitions: [
        {
          fromState: 'active',
          toState: 'dormant',
          reasonType: 'decay',
          reasonData: 'retention dropped below 0.5',
          timestamp: '2026-05-07T10:00:00Z',
        },
        {
          fromState: 'dormant',
          toState: 'active',
          reasonType: 'promote',
          reasonData: null,
          timestamp: '2026-05-08T10:00:00Z',
        },
      ],
    };
    vi.spyOn(api.memories, 'changelog').mockResolvedValue(fixture);

    const user = userEvent.setup();
    renderPanel();

    const summary = screen.getByText(/edit history/i);
    await user.click(summary);

    expect(await screen.findByText(/decay/)).toBeInTheDocument();
    expect(screen.getByText(/promote/)).toBeInTheDocument();
    expect(screen.getByText(/retention dropped below 0.5/)).toBeInTheDocument();
    expect(screen.getByText('(2)')).toBeInTheDocument();
  });

  it('shows the empty state when there are no transitions', async () => {
    vi.spyOn(api.memories, 'changelog').mockResolvedValue({
      memoryId,
      memoryContent: 'Test',
      totalTransitions: 0,
      transitions: [],
    });

    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByText(/edit history/i));
    expect(await screen.findByText(/no state transitions/i)).toBeInTheDocument();
  });

  it('shows error panel with retry when the request fails', async () => {
    vi.spyOn(api.memories, 'changelog').mockRejectedValue(new Error('Backend exploded'));

    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByText(/edit history/i));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    // Retry button is present.
    expect(screen.getByRole('button', { name: /try again/i })).toBeInTheDocument();
  });
});
