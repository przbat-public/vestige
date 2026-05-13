import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { ConfidenceResult } from '@/types';
import { DoubtList } from './DoubtList';

type DoubtRow = NonNullable<ConfidenceResult['results']>[number];

function makeRow(overrides: Partial<DoubtRow>): DoubtRow {
  return {
    id: 'aaa',
    content: 'Doubtful memory',
    confidence: 0.3,
    classification: 'opinion',
    ...overrides,
  } as DoubtRow;
}

function renderList(rows: DoubtRow[]) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ToastProvider>
            <DoubtList results={rows} />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('DoubtList', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders one row per result with confidence and classification', () => {
    renderList([
      makeRow({ id: 'aaa', content: 'First', confidence: 0.32 }),
      makeRow({ id: 'bbb', content: 'Second', confidence: 0.51, classification: 'observation' }),
    ]);
    expect(screen.getByText('First')).toBeInTheDocument();
    expect(screen.getByText('Second')).toBeInTheDocument();
    expect(screen.getByText('32%')).toBeInTheDocument();
    expect(screen.getByText('51%')).toBeInTheDocument();
    expect(screen.getByText('observation')).toBeInTheDocument();
  });

  it('renders nothing (no markup) when there are no results', () => {
    const { container } = renderList([]);
    expect(container.firstChild).toBeNull();
  });

  it('limit caps the number of rendered rows', () => {
    const rows = Array.from({ length: 10 }, (_, i) => makeRow({ id: `id-${i}`, content: `Doubt ${i}` }));
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, gcTime: 0 } },
    });
    render(
      <I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}>
          <MemoryRouter>
            <ToastProvider>
              <DoubtList results={rows} limit={3} />
            </ToastProvider>
          </MemoryRouter>
        </QueryClientProvider>
      </I18nextProvider>,
    );
    expect(screen.getAllByRole('button', { name: /verify/i })).toHaveLength(3);
  });

  it('Verify calls promote API and hides that row', async () => {
    const user = userEvent.setup();
    const promoteSpy = vi.spyOn(api.memories, 'promote').mockResolvedValue({ id: 'aaa' } as never);

    renderList([makeRow({ id: 'aaa', content: 'First doubtful' }), makeRow({ id: 'bbb', content: 'Second doubtful' })]);

    await user.click(screen.getAllByRole('button', { name: /verify/i })[0]);

    await waitFor(() => expect(promoteSpy).toHaveBeenCalledWith('aaa'));
    await waitFor(() => expect(screen.queryByText('First doubtful')).not.toBeInTheDocument());
    expect(screen.getByText('Second doubtful')).toBeInTheDocument();
  });

  it('Demote calls demote API and hides that row', async () => {
    const user = userEvent.setup();
    const demoteSpy = vi.spyOn(api.memories, 'demote').mockResolvedValue({ id: 'bbb' } as never);

    renderList([makeRow({ id: 'bbb', content: 'Will be demoted' })]);

    await user.click(screen.getByRole('button', { name: /demote/i }));

    await waitFor(() => expect(demoteSpy).toHaveBeenCalledWith('bbb'));
    await waitFor(() => expect(screen.queryByText('Will be demoted')).not.toBeInTheDocument());
  });
});
