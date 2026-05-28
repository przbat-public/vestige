import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { IntentionItem, IntentionListResponse, PredictResponse } from '@/types';
import { IntentionsPage } from './IntentionsPage';

function makeIntention(overrides: Partial<IntentionItem> = {}): IntentionItem {
  return {
    id: 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa',
    content: 'Investigate cashper callback gap',
    triggerType: 'context',
    triggerValue: 'cashper',
    status: 'active',
    priority: 'high',
    createdAt: '2026-05-01T10:00:00Z',
    ...overrides,
  };
}

function makeList(items: IntentionItem[], filter = 'active'): IntentionListResponse {
  return { intentions: items, total: items.length, filter };
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
            <IntentionsPage />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('IntentionsPage', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    // `predict` is fetched in parallel with `intentions` — we don't care
    // about its content here, just that it doesn't blow up the render.
    vi.spyOn(api, 'predict').mockResolvedValue({
      predictions: [],
      basedOn: 'test',
    } as PredictResponse);
  });

  it('shows fulfil/snooze/cancel buttons for an active intention', async () => {
    vi.spyOn(api, 'intentions').mockResolvedValue(makeList([makeIntention()]));

    renderPage();
    expect(await screen.findByRole('button', { name: /mark done|wykonane/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /snooze|odłóż/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /cancel|anuluj/i })).toBeInTheDocument();
  });

  it('shows only the reactivate button for a fulfilled intention', async () => {
    vi.spyOn(api, 'intentions').mockResolvedValue(makeList([makeIntention({ status: 'fulfilled' })], 'fulfilled'));

    renderPage();
    expect(await screen.findByRole('button', { name: /reactivate|przywróć/i })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /mark done|wykonane/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /snooze|odłóż/i })).not.toBeInTheDocument();
  });

  it('calls updateIntention with "fulfilled" when the complete button is clicked', async () => {
    const item = makeIntention({ id: 'target-id' });
    vi.spyOn(api, 'intentions').mockResolvedValue(makeList([item]));
    const updateSpy = vi.spyOn(api, 'updateIntention').mockResolvedValue({
      id: item.id,
      status: 'fulfilled',
      updated: true,
    });

    renderPage();
    const completeBtn = await screen.findByRole('button', { name: /mark done|wykonane/i });
    await userEvent.click(completeBtn);

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith('target-id', 'fulfilled');
    });
  });

  it('falls back to trigger type alone when triggerValue is empty', async () => {
    // Backend-created intentions through the MCP layer often arrive with
    // an empty triggerValue. The dashboard used to render "context: " with
    // an orphan colon — guard against the regression.
    vi.spyOn(api, 'intentions').mockResolvedValue(makeList([makeIntention({ triggerValue: '' })]));

    renderPage();
    // The colon belongs to the "with value" branch — assert it's NOT rendered.
    expect(await screen.findByText(/context|kontekstowy/i)).toBeInTheDocument();
    expect(screen.queryByText(/context:\s*$/i)).not.toBeInTheDocument();
  });
});
