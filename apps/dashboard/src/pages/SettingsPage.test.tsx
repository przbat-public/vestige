import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { ConfidenceResult, HealthCheck, RetentionDistribution, SystemStats } from '@/types';
import { SettingsPage } from './SettingsPage';

// SettingsPage hits a few read-only endpoints on mount; stub them to keep the
// test focused on the Doubt Mode (confidence audit) interactions.
const STATS: SystemStats = {
  totalMemories: 100,
  dueForReview: 10,
  averageRetention: 0.7,
  averageStorageStrength: 1.0,
  averageRetrievalStrength: 0.9,
  withEmbeddings: 95,
  embeddingCoverage: 95,
  embeddingModel: 'nomic-embed-text-v1.5',
};
const HEALTH: HealthCheck = {
  status: 'healthy',
  totalMemories: 100,
  averageRetention: 0.7,
  version: '3.2.0',
};
const DISTRIBUTION: RetentionDistribution = {
  distribution: [],
  endangered: [],
  byType: {},
  total: 0,
};

function setupReadOnlyMocks() {
  vi.spyOn(api, 'stats').mockResolvedValue(STATS);
  vi.spyOn(api, 'health').mockResolvedValue(HEALTH);
  vi.spyOn(api, 'retentionDistribution').mockResolvedValue(DISTRIBUTION);
}

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ToastProvider>
            <SettingsPage />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('SettingsPage Doubt Mode', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setupReadOnlyMocks();
  });

  it('runs confidence audit and renders Verify/Demote per result', async () => {
    vi.spyOn(api, 'confidence').mockResolvedValue({
      results: [
        {
          id: 'aaa',
          content: 'Memory with shaky confidence',
          confidence: 0.32,
          classification: 'opinion',
        },
        {
          id: 'bbb',
          content: 'Another doubtful memory',
          confidence: 0.45,
          classification: 'observation',
        },
      ],
    } as ConfidenceResult);

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /run audit/i }));

    expect(await screen.findByText(/Memory with shaky confidence/i)).toBeInTheDocument();
    expect(screen.getByText(/Another doubtful memory/i)).toBeInTheDocument();

    // Each row has its own Verify and Demote — there are 2 rows so 2 of each.
    const verifyButtons = screen.getAllByRole('button', { name: /verify/i });
    const demoteButtons = screen.getAllByRole('button', { name: /demote/i });
    expect(verifyButtons).toHaveLength(2);
    expect(demoteButtons).toHaveLength(2);
  });

  it('Verify button calls promote API and hides the row', async () => {
    vi.spyOn(api, 'confidence').mockResolvedValue({
      results: [
        {
          id: 'aaa',
          content: 'Doubtful memory',
          confidence: 0.3,
          classification: 'opinion',
        },
      ],
    } as ConfidenceResult);
    const promoteSpy = vi
      .spyOn(api.memories, 'promote')
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      .mockResolvedValue({ id: 'aaa' } as any);

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /run audit/i }));
    await screen.findByText(/Doubtful memory/i);

    await user.click(screen.getByRole('button', { name: /verify/i }));

    await waitFor(() => expect(promoteSpy).toHaveBeenCalledWith('aaa'));
    // Row hidden after action.
    await waitFor(() =>
      expect(screen.queryByText(/Doubtful memory/i)).not.toBeInTheDocument(),
    );
  });

  it('Demote button calls demote API and hides the row', async () => {
    vi.spyOn(api, 'confidence').mockResolvedValue({
      results: [
        {
          id: 'bbb',
          content: 'Another doubtful',
          confidence: 0.4,
          classification: 'opinion',
        },
      ],
    } as ConfidenceResult);
    const demoteSpy = vi
      .spyOn(api.memories, 'demote')
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      .mockResolvedValue({ id: 'bbb' } as any);

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /run audit/i }));
    await screen.findByText(/Another doubtful/i);

    await user.click(screen.getByRole('button', { name: /demote/i }));

    await waitFor(() => expect(demoteSpy).toHaveBeenCalledWith('bbb'));
    await waitFor(() =>
      expect(screen.queryByText(/Another doubtful/i)).not.toBeInTheDocument(),
    );
  });
});
