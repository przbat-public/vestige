import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import { MaintenancePanel } from './MaintenancePanel';

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <ToastProvider>
          <MaintenancePanel />
        </ToastProvider>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('MaintenancePanel', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders all four maintenance actions by default', () => {
    renderPanel();
    expect(screen.getByText(/regenerate embeddings/i)).toBeInTheDocument();
    expect(screen.getByText(/find near-duplicates/i)).toBeInTheDocument();
    expect(screen.getByText(/garbage collection/i)).toBeInTheDocument();
    expect(screen.getByText(/backup database now/i)).toBeInTheDocument();
  });

  it('regenerate-embeddings button calls api with force=false and shows the result', async () => {
    const spy = vi
      .spyOn(api.maintenance, 'regenerateEmbeddings')
      .mockResolvedValue({
        successful: 12,
        failed: 0,
        skipped: 100,
        errors: [],
        durationMs: 350,
      });

    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole('button', { name: /^regenerate$/i }));

    await waitFor(() => expect(spy).toHaveBeenCalledWith({ force: false }));
    expect(await screen.findByText(/regenerated 12, skipped 100, failed 0/i)).toBeInTheDocument();
  });

  it('find-duplicates button shows cluster summary inline', async () => {
    vi.spyOn(api.maintenance, 'findDuplicates').mockResolvedValue({
      clusters: [
        {
          clusterId: 0,
          size: 3,
          members: [
            { id: 'a', contentPreview: 'Card alpha…', similarityToAnchor: '1.000', retention: 0.7, createdAt: '', tags: [] },
            { id: 'b', contentPreview: 'Card alpha-prime…', similarityToAnchor: '0.91', retention: 0.6, createdAt: '', tags: [] },
            { id: 'c', contentPreview: 'Card alpha-beta…', similarityToAnchor: '0.88', retention: 0.5, createdAt: '', tags: [] },
          ],
          suggestedAction: 'merge',
        },
      ],
      totalMemories: 100,
      totalWithEmbeddings: 90,
      totalClusters: 1,
      threshold: 0.85,
    });

    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole('button', { name: /scan for duplicates/i }));

    expect(await screen.findByText(/1 clusters across 90 memories/i)).toBeInTheDocument();
    expect(screen.getByText(/cluster of 3/i)).toBeInTheDocument();
    expect(screen.getByText(/Card alpha…/)).toBeInTheDocument();
  });

  it('GC preview shows candidates and the destructive button only after preview', async () => {
    vi.spyOn(api.maintenance, 'gc').mockResolvedValue({
      dryRun: true,
      candidateCount: 5,
      sample: [
        { id: 'aaaaaaaa', retention: 0.05, ageDays: 90, contentPreview: 'old memory…' },
      ],
    });

    const user = userEvent.setup();
    renderPanel();

    // Destructive button should NOT be visible before preview runs.
    expect(screen.queryByRole('button', { name: /^delete /i })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /preview candidates/i }));

    expect(await screen.findByText(/5 memories below the retention threshold/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^delete 5/i })).toBeInTheDocument();
  });

  it('GC delete asks for confirmation before calling the destructive API', async () => {
    const gcSpy = vi.spyOn(api.maintenance, 'gc').mockResolvedValueOnce({
      dryRun: true,
      candidateCount: 3,
    });

    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole('button', { name: /preview candidates/i }));
    await screen.findByText(/3 memories below the retention threshold/i);

    // First confirm: user cancels — no second call.
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);
    await user.click(screen.getByRole('button', { name: /^delete 3/i }));
    expect(confirmSpy).toHaveBeenCalled();
    expect(gcSpy).toHaveBeenCalledTimes(1); // Only the preview, no delete.

    // Second confirm: user accepts — call goes through.
    confirmSpy.mockReturnValue(true);
    gcSpy.mockResolvedValueOnce({ dryRun: false, candidateCount: 3, deleted: 3 });
    await user.click(screen.getByRole('button', { name: /^delete 3/i }));

    await waitFor(() => expect(gcSpy).toHaveBeenCalledTimes(2));
    expect(gcSpy.mock.calls[1][0]).toEqual({ dry_run: false, min_retention: 0.1 });
  });

  it('backup button calls api and shows the saved path', async () => {
    vi.spyOn(api.maintenance, 'backup').mockResolvedValue({
      path: '/Users/me/.vestige/backups/vestige-20260507.db',
      sizeBytes: 1024 * 1024 * 5,
      timestamp: '2026-05-07T16:00:00Z',
    });

    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole('button', { name: /^create backup$/i }));

    expect(await screen.findByText(/saved to .*vestige-20260507\.db/i)).toBeInTheDocument();
  });

  it('shows error toast when an action fails', async () => {
    vi.spyOn(api.maintenance, 'backup').mockRejectedValue(new Error('Disk full'));
    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByRole('button', { name: /^create backup$/i }));
    expect(await screen.findByText(/disk full/i)).toBeInTheDocument();
  });
});
