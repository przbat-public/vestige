import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { Memory } from '@/types';
import { MemoryDetail } from './MemoryDetail';

const baseMemory: Memory = {
  id: '11111111-1111-4111-9111-111111111111',
  content: 'Original content',
  nodeType: 'fact',
  tags: ['alpha', 'beta'],
  retentionStrength: 0.7,
  storageStrength: 1.0,
  retrievalStrength: 0.9,
  createdAt: '2026-04-01T10:00:00Z',
  updatedAt: '2026-05-01T10:00:00Z',
  reviewCount: 3,
  // Required by the wire DTO; defaults match what the server would
  // infer for a plain `fact` with no special tags.
  epistemicStatus: 'world',
  memorySystem: 'semantic',
};

function renderDetail(overrides: Partial<{ memory: Memory; onUpdate: () => void; onClose: () => void }> = {}) {
  const memory = overrides.memory ?? baseMemory;
  const onUpdate = overrides.onUpdate ?? vi.fn();
  const onClose = overrides.onClose ?? vi.fn();
  // Each test gets a fresh client to avoid cross-test cache leaks; retry off
  // and gcTime small so failures surface fast.
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return {
    onUpdate,
    onClose,
    ...render(
      <I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}>
          <MemoryRouter>
            <ToastProvider>
              <MemoryDetail memory={memory} onUpdate={onUpdate} onClose={onClose} />
            </ToastProvider>
          </MemoryRouter>
        </QueryClientProvider>
      </I18nextProvider>,
    ),
  };
}

describe('MemoryDetail', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders memory content and tags by default', () => {
    renderDetail();
    expect(screen.getByText('Original content')).toBeInTheDocument();
    expect(screen.getByText('alpha')).toBeInTheDocument();
    expect(screen.getByText('beta')).toBeInTheDocument();
  });

  it('enters edit mode when Edit is clicked and shows form fields', async () => {
    const user = userEvent.setup();
    renderDetail();
    await user.click(screen.getByRole('button', { name: /edit/i }));

    const textarea = screen.getByLabelText(/memory content/i);
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveValue('Original content');

    const tagsInput = screen.getByLabelText(/^tags$/i);
    expect(tagsInput).toHaveValue('alpha, beta');
  });

  it('cancels edit and restores original content', async () => {
    const user = userEvent.setup();
    renderDetail();
    await user.click(screen.getByRole('button', { name: /edit/i }));

    const textarea = screen.getByLabelText(/memory content/i);
    await user.clear(textarea);
    await user.type(textarea, 'Edited but not saved');

    await user.click(screen.getByRole('button', { name: /cancel/i }));
    expect(screen.queryByLabelText(/memory content/i)).not.toBeInTheDocument();
    expect(screen.getByText('Original content')).toBeInTheDocument();
  });

  it('submits content+tags to api.memories.update on Save', async () => {
    const user = userEvent.setup();
    // PATCH resolves with `MemoryUpdateResultDto { memory, field }`.
    const updateSpy = vi.spyOn(api.memories, 'update').mockResolvedValue({
      memory: { ...baseMemory, content: 'Updated content', tags: ['gamma'] },
      field: 'content+tags',
    });

    const { onUpdate } = renderDetail();
    await user.click(screen.getByRole('button', { name: /edit/i }));

    const textarea = screen.getByLabelText(/memory content/i);
    await user.clear(textarea);
    await user.type(textarea, 'Updated content');

    const tagsInput = screen.getByLabelText(/^tags$/i);
    await user.clear(tagsInput);
    await user.type(tagsInput, 'gamma');

    await user.click(screen.getByRole('button', { name: /save/i }));

    await waitFor(() =>
      expect(updateSpy).toHaveBeenCalledWith(baseMemory.id, {
        content: 'Updated content',
        tags: ['gamma'],
      }),
    );
    await waitFor(() => expect(onUpdate).toHaveBeenCalled());
  });

  it('renders the saved content after a successful save (no stale snapshot)', async () => {
    // Regression for the wire drift that made Save look like a no-op: the
    // mutation resolves with `{ memory, field }`, the hook writes
    // `memory` under `queryKeys.memory(id)`, and this panel must read that
    // cache entry instead of the stale list snapshot it was handed.
    const user = userEvent.setup();
    vi.spyOn(api.memories, 'update').mockResolvedValue({
      memory: { ...baseMemory, content: 'Fresh from the server' },
      field: 'content',
    });

    renderDetail();
    await user.click(screen.getByRole('button', { name: /edit/i }));
    const textarea = screen.getByLabelText(/memory content/i);
    await user.clear(textarea);
    await user.type(textarea, 'Fresh from the server');
    await user.click(screen.getByRole('button', { name: /save/i }));

    await waitFor(() => expect(screen.getByText('Fresh from the server')).toBeInTheDocument());
    expect(screen.queryByText('Original content')).not.toBeInTheDocument();
  });

  it('does not submit when content has not changed', async () => {
    const user = userEvent.setup();
    const updateSpy = vi.spyOn(api.memories, 'update');
    renderDetail();
    await user.click(screen.getByRole('button', { name: /edit/i }));
    await user.click(screen.getByRole('button', { name: /save/i }));
    expect(updateSpy).not.toHaveBeenCalled();
  });

  it('shows "due now" badge when nextReviewAt is in the past', () => {
    renderDetail({
      memory: { ...baseMemory, nextReviewAt: '2020-01-01T00:00:00Z' },
    });
    expect(screen.getByText(/due now/i)).toBeInTheDocument();
  });

  it('shows a relative time badge when nextReviewAt is in the future', () => {
    const future = new Date(Date.now() + 5 * 86_400_000).toISOString();
    renderDetail({
      memory: { ...baseMemory, nextReviewAt: future },
    });
    // Assert via the badge's accessible name — the visible text varies by
    // locale but the aria-label uses our deterministic translation key.
    const badge = screen.getByLabelText(/Next FSRS review in 5 days/i);
    expect(badge).toBeInTheDocument();
    expect(badge.textContent).toMatch(/next review/i);
  });

  it('omits the next-review badge when nextReviewAt is missing', () => {
    renderDetail({
      memory: { ...baseMemory, nextReviewAt: undefined },
    });
    expect(screen.queryByText(/next review/i)).not.toBeInTheDocument();
  });
});
