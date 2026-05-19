import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { ConfidenceResult, DreamResult, ReflectResult } from '@/types';
import { BriefingPage } from './BriefingPage';

const REFLECT: ReflectResult = {
  status: 'reflected',
  // `focus` is `string | undefined` on the wire DTO — the server omits
  // it (rather than emitting `null`) when there's no focus filter.
  focus: undefined,
  depth: 'standard',
  summary: 'Memory base is broadly consistent.',
  insights: ['No memories about deployment process', 'Two memories disagree on deadline'],
  structuredInsights: [
    {
      type: 'knowledge_gap',
      description: 'No memories about deployment process',
      severity: 'medium',
      sourceMemoryIds: [],
    },
    {
      type: 'contradiction',
      description: 'Two memories disagree on deadline',
      severity: 'high',
      sourceMemoryIds: [],
    },
  ],
  memoriesAnalyzed: 0,
};

const CONFIDENCE: ConfidenceResult = {
  results: [{ id: 'aaa', content: 'Doubtful memory', confidence: 0.3, classification: 'opinion' }],
} as ConfidenceResult;

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ToastProvider>
            <BriefingPage />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('BriefingPage', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('auto-fetches and renders reflection insights on mount', async () => {
    vi.spyOn(api, 'reflect').mockResolvedValue(REFLECT);
    vi.spyOn(api, 'confidence').mockResolvedValue(CONFIDENCE);

    renderPage();

    expect(await screen.findByText(/Memory base is broadly consistent/i)).toBeInTheDocument();
    expect(screen.getByText(/No memories about deployment process/i)).toBeInTheDocument();
    expect(screen.getByText(/Two memories disagree on deadline/i)).toBeInTheDocument();
  });

  it('auto-fetches and renders Doubt-mode rows on mount', async () => {
    vi.spyOn(api, 'reflect').mockResolvedValue(REFLECT);
    vi.spyOn(api, 'confidence').mockResolvedValue(CONFIDENCE);

    renderPage();

    expect(await screen.findByText('Doubtful memory')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /verify/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /demote/i })).toBeInTheDocument();
  });

  it('Dream button is gated and only fires api.dream on explicit click', async () => {
    vi.spyOn(api, 'reflect').mockResolvedValue(REFLECT);
    vi.spyOn(api, 'confidence').mockResolvedValue(CONFIDENCE);
    // DreamResultPanel reads several arrays/numbers from the result, so the
    // mock must satisfy the same shape the backend returns.
    const dreamSpy = vi.spyOn(api, 'dream').mockResolvedValue({
      memoriesReplayed: 50,
      connectionsPersisted: 12,
      insights: [],
      contradictions: [],
      memoriesDemoted: [],
      stats: { duration_ms: 800 },
    } as unknown as DreamResult);

    renderPage();
    await screen.findByText(/Memory base is broadly consistent/i);

    expect(dreamSpy).not.toHaveBeenCalled();

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /dream now/i }));

    await waitFor(() => expect(dreamSpy).toHaveBeenCalled());
  });

  it('shows a toast and does not crash when reflect fails', async () => {
    vi.spyOn(api, 'reflect').mockRejectedValue(new Error('reflect blew up'));
    vi.spyOn(api, 'confidence').mockResolvedValue(CONFIDENCE);

    renderPage();
    expect(await screen.findByText(/reflect blew up/i)).toBeInTheDocument();
    // Doubt section still renders independently — error in one section
    // should not blow away the others.
    expect(screen.getByText('Doubtful memory')).toBeInTheDocument();
  });
});
