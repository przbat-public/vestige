import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { useDialogStore } from '@/stores/dialogs';
import { ToastProvider } from '@/stores/toast';
import type { ConfidenceResult, HealthCheck, ReflectResult, RetentionDistribution, SystemStats } from '@/types';
import { SettingsPage } from './SettingsPage';

const mockNavigate = vi.fn();
vi.mock('react-router', async () => {
  const actual = (await vi.importActual<typeof import('react-router')>('react-router')) as Record<string, unknown>;
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

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
    mockNavigate.mockReset();
    useDialogStore.setState({ pendingSelectMemoryId: null });
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
    const promoteSpy = vi.spyOn(api.memories, 'promote').mockResolvedValue({ id: 'aaa' } as never);

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /run audit/i }));
    await screen.findByText(/Doubtful memory/i);

    await user.click(screen.getByRole('button', { name: /verify/i }));

    await waitFor(() => expect(promoteSpy).toHaveBeenCalledWith('aaa'));
    // Row hidden after action.
    await waitFor(() => expect(screen.queryByText(/Doubtful memory/i)).not.toBeInTheDocument());
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
    const demoteSpy = vi.spyOn(api.memories, 'demote').mockResolvedValue({ id: 'bbb' } as never);

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /run audit/i }));
    await screen.findByText(/Another doubtful/i);

    await user.click(screen.getByRole('button', { name: /demote/i }));

    await waitFor(() => expect(demoteSpy).toHaveBeenCalledWith('bbb'));
    await waitFor(() => expect(screen.queryByText(/Another doubtful/i)).not.toBeInTheDocument());
  });

  it('makes the audit-result content open the host memory drawer', async () => {
    // Before this fix, the doubt-mode list showed each candidate as
    // dead text. The user could promote or demote it but couldn't
    // first read the full memory or check its neighborhood — the
    // exact thing a "are you sure about this?" workflow demands.
    // Contract: clicking the content stages the id in the dialog
    // store and navigates to /memories so the drawer opens.
    vi.spyOn(api, 'confidence').mockResolvedValue({
      results: [
        {
          id: 'doubtful-id-zzz',
          content: 'Confidence-shaky claim about Redis vs Valkey',
          confidence: 0.31,
          classification: 'opinion',
        },
      ],
    } as ConfidenceResult);

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /run audit/i }));

    const trigger = await screen.findByRole('button', {
      name: /Confidence-shaky claim about Redis vs Valkey/i,
    });
    await user.click(trigger);

    expect(useDialogStore.getState().pendingSelectMemoryId).toBe('doubtful-id-zzz');
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });
});

describe('SettingsPage reflect insights', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    mockNavigate.mockReset();
    useDialogStore.setState({ pendingSelectMemoryId: null });
    setupReadOnlyMocks();
  });

  it('renders sourceMemoryIds as clickable chips that open the host drawer', async () => {
    // ReflectInsightDto carries `sourceMemoryIds` precisely so the UI
    // can route the user back to the memories that produced the
    // insight. The original render dropped the field on the floor —
    // a contradiction would surface ("two memories disagree about X")
    // but the user couldn't open either. That's an unsalvageable
    // information state for a metacognitive tool. Contract: each id
    // renders as a focusable chip; clicking routes to /memories with
    // the id staged on the dialog store.
    vi.spyOn(api, 'reflect').mockResolvedValue({
      status: 'reflected',
      depth: 'standard',
      memoriesAnalyzed: 42,
      insights: [],
      structuredInsights: [
        {
          type: 'contradiction',
          description: 'Two memories disagree about deployment cadence',
          sourceMemoryIds: ['mem-source-aaa', 'mem-source-bbb'],
          severity: 'high',
        },
      ],
      summary: 'Found one contradiction',
    } as ReflectResult);

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /run reflect/i }));

    const chipA = await screen.findByRole('button', { name: /mem-source-aaa/i });
    const chipB = await screen.findByRole('button', { name: /mem-source-bbb/i });
    expect(chipA).toBeInTheDocument();
    expect(chipB).toBeInTheDocument();

    await user.click(chipA);
    expect(useDialogStore.getState().pendingSelectMemoryId).toBe('mem-source-aaa');
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });
});
