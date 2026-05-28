import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import { AddMemoryDialog } from './AddMemoryDialog';

type SmartIngestResult = Awaited<ReturnType<typeof api.memories.smartIngest>>;

function makeResult(overrides: Partial<SmartIngestResult> = {}): SmartIngestResult {
  return {
    success: true,
    decision: 'create',
    nodeId: 'new-mem-123',
    message: 'Created',
    hasEmbedding: true,
    predictionError: 0.1,
    importanceScore: 0.7,
    reason: 'fresh memory',
    explanation: 'New memory created',
    ...overrides,
  };
}

function renderDialog(onClose: () => void = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <AddMemoryDialog open={true} onClose={onClose} />
        </ToastProvider>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('AddMemoryDialog', () => {
  beforeAll(async () => {
    await i18n.changeLanguage('en');
    await i18n.loadNamespaces(['translation']);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('submits the form, calls smartIngest with parsed tags, and closes the dialog on plain success', async () => {
    const ingest = vi.spyOn(api.memories, 'smartIngest').mockResolvedValue(makeResult());
    const onClose = vi.fn();
    const user = userEvent.setup();
    renderDialog(onClose);

    const textarea = screen.getByLabelText(/content/i);
    await user.type(textarea, 'Vestige uses FSRS-6 for spaced repetition');

    const tagsInput = screen.getByLabelText(/tags/i);
    await user.type(tagsInput, 'vestige, fsrs , architecture');

    await user.click(screen.getByRole('button', { name: /save memory/i }));

    await waitFor(() => expect(ingest).toHaveBeenCalledTimes(1));
    const arg = ingest.mock.calls[0][0];
    expect(arg.content).toBe('Vestige uses FSRS-6 for spaced repetition');
    // The "fsrs " entry has trailing whitespace from the comma split —
    // the dialog must trim it before the API sees it, otherwise the
    // backend silently stores "fsrs " as a distinct tag from "fsrs".
    expect(arg.tags).toEqual(['vestige', 'fsrs', 'architecture']);
    expect(onClose).toHaveBeenCalled();
  });

  it('keeps the dialog open and surfaces a compound_content_warning so the user can split or force', async () => {
    // Smart ingest returns success but flags the content as compound.
    // The original implementation closed the dialog unconditionally,
    // burying this warning behind a toast that the user never sees —
    // forcing them to navigate to the memory just to read advice they
    // need *before* deciding to keep their write. The warning is
    // actionable (split or check `Force create`), so the dialog must
    // stay mounted.
    vi.spyOn(api.memories, 'smartIngest').mockResolvedValue(
      makeResult({
        decision: 'create',
        compound_content_warning:
          'Content covers multiple topics (decision, preference). Consider splitting into atomic memories.',
      }),
    );
    const onClose = vi.fn();
    const user = userEvent.setup();
    renderDialog(onClose);

    await user.type(
      screen.getByLabelText(/content/i),
      'We decided to use PostgreSQL. Also John prefers dark mode. Deadline Friday.',
    );
    await user.click(screen.getByRole('button', { name: /save memory/i }));

    await waitFor(() =>
      expect(screen.getByText(/multiple topics .* Consider splitting into atomic memories/i)).toBeInTheDocument(),
    );
    // Dialog stays open — the user needs to act on the warning.
    expect(onClose).not.toHaveBeenCalled();
  });

  it('does not call smartIngest when the content is empty (client-side validation)', async () => {
    const ingest = vi.spyOn(api.memories, 'smartIngest').mockResolvedValue(makeResult());
    const onClose = vi.fn();
    const user = userEvent.setup();
    renderDialog(onClose);

    await user.click(screen.getByRole('button', { name: /save memory/i }));

    // zod schema requires content.min(1) — submission should be blocked.
    await waitFor(() => expect(screen.getByText(/required|cannot be empty/i)).toBeInTheDocument());
    expect(ingest).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });
});
