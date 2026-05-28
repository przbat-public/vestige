import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import { TimelinePage } from './TimelinePage';

function setupMock(memoryCounts: number[]) {
  vi.spyOn(api, 'timeline').mockResolvedValue({
    timeline: memoryCounts.map((count, idx) => ({
      date: `2026-05-${(22 - idx).toString().padStart(2, '0')}`,
      count,
      memories: Array.from({ length: Math.min(count, 1) }, (_, i) => ({
        id: `m-${idx}-${i}`,
        content: `memory ${idx}-${i}`,
        nodeType: 'fact',
        retentionStrength: 0.5,
        createdAt: `2026-05-${(22 - idx).toString().padStart(2, '0')}T00:00:00Z`,
      })),
    })),
  });
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ToastProvider>
            <TimelinePage />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('TimelinePage', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders a clean per-day count without orphaned " on " (English)', async () => {
    await i18n.changeLanguage('en');
    setupMock([1, 5]);
    renderPage();
    expect(await screen.findByText('1 memory')).toBeInTheDocument();
    expect(await screen.findByText('5 memories')).toBeInTheDocument();
    // The previous hack stripped " on " — regression guard.
    expect(screen.queryByText(/ on $/)).not.toBeInTheDocument();
  });

  it('uses Polish plural forms (one/few/many) for the per-day count', async () => {
    await i18n.changeLanguage('pl');
    // 1 = "one" → "1 wspomnienie"; 3 = "few" → "3 wspomnienia"; 5 = "many" → "5 wspomnień"
    setupMock([1, 3, 5]);
    renderPage();
    expect(await screen.findByText('1 wspomnienie')).toBeInTheDocument();
    expect(await screen.findByText('3 wspomnienia')).toBeInTheDocument();
    expect(await screen.findByText('5 wspomnień')).toBeInTheDocument();
    // Regression guard: the old code rendered "5 wspomnień dnia " (with
    // dangling preposition + trailing space) in Polish.
    expect(screen.queryByText(/dnia\s*$/)).not.toBeInTheDocument();
  });
});
