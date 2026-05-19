import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { ReviewItem, ReviewQueueResponse, ReviewResult } from '@/types';
import { ReviewPage } from './ReviewPage';

function makeItem(overrides: Partial<ReviewItem> = {}): ReviewItem {
  return {
    id: '11111111-1111-4111-9111-111111111111',
    content: 'Why is the sky blue?',
    nodeType: 'fact',
    tags: ['physics'],
    retentionStrength: 0.6,
    storageStrength: 1.0,
    retrievalStrength: 0.8,
    createdAt: '2026-04-01T10:00:00Z',
    updatedAt: '2026-05-01T10:00:00Z',
    reviewCount: 1,
    difficulty: 5.0,
    stability: 1.5,
    epistemicStatus: 'world',
    memorySystem: 'semantic',
    ...overrides,
  };
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
            <ReviewPage />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('ReviewPage', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('shows the queue-empty state when nothing is due', async () => {
    vi.spyOn(api.review, 'queue').mockResolvedValue({
      total: 0,
      memories: [],
    } as ReviewQueueResponse);

    renderPage();
    expect(await screen.findByText(/nothing to review right now/i)).toBeInTheDocument();
  });

  it('renders the first card and progresses on Good (3) keyboard press', async () => {
    const a = makeItem({ id: 'aaa', content: 'Card A' });
    const b = makeItem({ id: 'bbb', content: 'Card B' });
    vi.spyOn(api.review, 'queue').mockResolvedValue({
      total: 2,
      memories: [a, b],
    } as ReviewQueueResponse);
    const reviewSpy = vi.spyOn(api.memories, 'review').mockResolvedValue({
      id: a.id,
      rating: 'good',
      previousRetention: 0.6,
      newRetention: 0.85,
      previousStability: 1.5,
      newStability: 4.0,
      difficulty: 5.0,
      reps: 2,
      lapses: 0,
      nextReviewAt: '2026-05-12T10:00:00Z',
    } satisfies ReviewResult);

    renderPage();
    expect(await screen.findByText('Card A')).toBeInTheDocument();

    const user = userEvent.setup();
    await user.keyboard('3');

    await waitFor(() => expect(reviewSpy).toHaveBeenCalledWith(a.id, 3));
    expect(await screen.findByText('Card B')).toBeInTheDocument();
  });

  it('rates via the visible button as well as keyboard', async () => {
    const card = makeItem({ id: 'aaa' });
    vi.spyOn(api.review, 'queue').mockResolvedValue({
      total: 1,
      memories: [card],
    } as ReviewQueueResponse);
    const reviewSpy = vi.spyOn(api.memories, 'review').mockResolvedValue({
      id: card.id,
      rating: 'easy',
      previousRetention: 0.6,
      newRetention: 0.95,
      previousStability: 1.5,
      newStability: 8.0,
      difficulty: 4.0,
      reps: 2,
      lapses: 0,
      // `nextReviewAt` is omitted by the server when FSRS-6 suppresses
      // the card; the wire DTO uses `Option<String>` + `skip_serializing_if`
      // so the dashboard sees `undefined`, never `null`.
      nextReviewAt: undefined,
    } satisfies ReviewResult);

    renderPage();
    await screen.findByText(card.content);

    const user = userEvent.setup();
    // The four buttons render with localized labels; pick by accessible name.
    const easyBtn = screen.getByRole('button', { name: /easy/i });
    await user.click(easyBtn);

    await waitFor(() => expect(reviewSpy).toHaveBeenCalledWith(card.id, 4));
  });

  it('shows the session-done view after the final card and offers refresh', async () => {
    const card = makeItem();
    vi.spyOn(api.review, 'queue').mockResolvedValue({
      total: 1,
      memories: [card],
    } as ReviewQueueResponse);
    vi.spyOn(api.memories, 'review').mockResolvedValue({
      id: card.id,
      rating: 'good',
      previousRetention: 0.6,
      newRetention: 0.8,
      previousStability: 1.5,
      newStability: 3.5,
      difficulty: 5.0,
      reps: 2,
      lapses: 0,
      nextReviewAt: undefined,
    } satisfies ReviewResult);

    renderPage();
    await screen.findByText(card.content);

    const user = userEvent.setup();
    await user.keyboard('3');

    expect(await screen.findByText(/session complete/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /check for more/i })).toBeInTheDocument();
  });

  it('does not trigger keyboard rating while typing in an input', async () => {
    const a = makeItem({ id: 'aaa' });
    vi.spyOn(api.review, 'queue').mockResolvedValue({
      total: 1,
      memories: [a],
    } as ReviewQueueResponse);
    const reviewSpy = vi.spyOn(api.memories, 'review');

    renderPage();
    await screen.findByText(a.content);

    // Mount an input that grabs focus, then type "3" — should not fire review.
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();

    const user = userEvent.setup();
    await user.keyboard('3');

    expect(reviewSpy).not.toHaveBeenCalled();
    document.body.removeChild(input);
  });
});
