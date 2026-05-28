import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { useDialogStore } from '@/stores/dialogs';
import { TemporalPage } from './TemporalPage';

const mockNavigate = vi.fn();
vi.mock('react-router', async () => {
  const actual = (await vi.importActual<typeof import('react-router')>('react-router')) as Record<string, unknown>;
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

vi.mock('@/stores/telemetry', () => ({
  useTrackPageView: () => undefined,
  track: () => undefined,
  EVENT: { temporal_invalidate: 'temporal_invalidate' },
}));

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <TemporalPage />
        </MemoryRouter>
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

describe('TemporalPage — open memory drawer', () => {
  beforeEach(() => {
    mockNavigate.mockReset();
    useDialogStore.setState({ pendingSelectMemoryId: null });
    vi.spyOn(api, 'temporal').mockResolvedValue({
      action: 'current',
      topic: undefined,
      count: 1,
      memories: [
        {
          id: 'abc-1234-5678',
          content: 'Redis caches mask stale data when TTLs are wrong.',
          retention: 0.82,
          tags: ['redis', 'caching'],
        },
      ],
    });
  });

  it('clicking the content of a temporal entry stages it in the dialog store and navigates to /memories', async () => {
    const user = userEvent.setup();
    renderPage();

    const openBtn = await screen.findByRole('button', {
      name: /Redis caches mask stale data/i,
    });
    await user.click(openBtn);

    await waitFor(() => {
      expect(useDialogStore.getState().pendingSelectMemoryId).toBe('abc-1234-5678');
    });
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });
});
