import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import { ConfirmDialogHost } from '@/components/ConfirmDialog';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { Memory, MemoryListResponse } from '@/types';
import { MemoriesPage } from './MemoriesPage';

function makeMemory(overrides: Partial<Memory>): Memory {
  return {
    id: 'mem-x',
    content: 'Memory content',
    nodeType: 'fact',
    tags: [],
    retentionStrength: 0.5,
    storageStrength: 1,
    retrievalStrength: 0.5,
    createdAt: '2026-04-01T00:00:00Z',
    // Record time (V17) — required by `MemoryDto`.
    recordedAt: '2026-04-01T00:00:00Z',
    updatedAt: '2026-04-01T00:00:00Z',
    reviewCount: 0,
    // The wire DTO requires both — server infers them from `nodeType` +
    // tags + content. Tests pick the most common defaults so they don't
    // have to think about it; override per-test when behavior depends on
    // it.
    epistemicStatus: 'world',
    memorySystem: 'semantic',
    ...overrides,
  };
}

const memories: Memory[] = [
  makeMemory({ id: 'aaa', content: 'Alpha memory' }),
  makeMemory({ id: 'bbb', content: 'Beta memory' }),
  makeMemory({ id: 'ccc', content: 'Gamma memory' }),
  makeMemory({ id: 'ddd', content: 'Delta memory' }),
];

function setupListMock() {
  vi.spyOn(api.memories, 'list').mockResolvedValue({
    total: memories.length,
    memories,
  } satisfies MemoryListResponse);
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
            <MemoriesPage />
            <ConfirmDialogHost />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('MemoriesPage — bulk selection', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setupListMock();
  });

  it('renders rows with hidden-by-default checkboxes', async () => {
    renderPage();
    await screen.findByText('Alpha memory');
    // Checkboxes exist in DOM but row container hides them via opacity-0
    // until hover or selection — they are still queryable.
    const checkboxes = screen.getAllByRole('checkbox');
    // 4 row checkboxes + 1 "select all" master = 5
    expect(checkboxes.length).toBeGreaterThanOrEqual(5);
  });

  it('clicking a row checkbox enters selection mode and shows the bulk toolbar', async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText('Alpha memory');

    // Aria-label is per-row + content-aware, so we don't depend on DOM order.
    const alphaCheckbox = screen.getByRole('checkbox', { name: /Alpha memory/i });
    await user.click(alphaCheckbox);

    const toolbar = await screen.findByRole('region', { name: /bulk action toolbar/i });
    // Some i18n backends split count placeholders across nodes — match by
    // the toolbar's accumulated textContent rather than getByText.
    await waitFor(() => expect(toolbar.textContent ?? '').toMatch(/1 selected/i));
  });

  it('Select-all master toggles every visible row', async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText('Alpha memory');

    const master = screen.getByRole('checkbox', { name: /select all visible memories/i });
    await user.click(master);

    const toolbar = await screen.findByRole('region', { name: /bulk action toolbar/i });
    await waitFor(() => expect(toolbar.textContent ?? '').toMatch(/4 selected/i));
  });

  it('promote bulk action calls api.memories.promote per id and toasts the success count', async () => {
    const user = userEvent.setup();
    const promoteSpy = vi.spyOn(api.memories, 'promote').mockResolvedValue(makeMemory({ id: 'aaa' }));

    renderPage();
    await screen.findByText('Alpha memory');

    await user.click(screen.getByRole('checkbox', { name: /Alpha memory/i }));
    await user.click(screen.getByRole('checkbox', { name: /Beta memory/i }));

    const toolbar = await screen.findByRole('region', { name: /bulk action toolbar/i });
    const promoteBtn = within(toolbar).getByRole('button', { name: /strengthen/i });
    await user.click(promoteBtn);

    await waitFor(() => expect(promoteSpy).toHaveBeenCalledTimes(2));
    expect(promoteSpy).toHaveBeenCalledWith('aaa');
    expect(promoteSpy).toHaveBeenCalledWith('bbb');
    expect(await screen.findByText(/strengthened 2 memories/i)).toBeInTheDocument();
  });

  it('partial failure path produces a partial-success toast', async () => {
    const user = userEvent.setup();
    let n = 0;
    vi.spyOn(api.memories, 'demote').mockImplementation(async () => {
      n++;
      if (n === 1) throw new Error('boom');
      return makeMemory({ id: 'mem-x' });
    });

    renderPage();
    await screen.findByText('Alpha memory');

    await user.click(screen.getByRole('checkbox', { name: /Alpha memory/i }));
    await user.click(screen.getByRole('checkbox', { name: /Beta memory/i }));

    const toolbar = await screen.findByRole('region', { name: /bulk action toolbar/i });
    const demoteBtn = within(toolbar).getByRole('button', { name: /weaken/i });
    await user.click(demoteBtn);

    expect(await screen.findByText(/applied to 1.*1 failed/i)).toBeInTheDocument();
  });

  it('Esc clears selection and hides the toolbar', async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText('Alpha memory');

    await user.click(screen.getByRole('checkbox', { name: /Alpha memory/i }));
    expect(await screen.findByRole('region', { name: /bulk action toolbar/i })).toBeInTheDocument();

    // Move focus to <body> so the window-level Escape handler runs (it
    // ignores Escape while inside an input — that's the desired UX, not a bug).
    document.body.focus();
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('region', { name: /bulk action toolbar/i })).not.toBeInTheDocument());
  });

  it('Cmd+A selects every visible row', async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText('Alpha memory');

    document.body.focus();
    await user.keyboard('{Meta>}a{/Meta}');

    const toolbar = await screen.findByRole('region', { name: /bulk action toolbar/i });
    await waitFor(() => expect(toolbar.textContent ?? '').toMatch(/4 selected/i));
  });

  it('shows pagination controls when total exceeds the page size and calls list with the right offset', async () => {
    const user = userEvent.setup();

    // Total of 200, page of 100. UI must render Previous (disabled) + Next.
    const page1: Memory[] = Array.from({ length: 100 }, (_, i) =>
      makeMemory({ id: `p1-${i}`, content: `Page1 mem ${i}` }),
    );
    const page2: Memory[] = Array.from({ length: 100 }, (_, i) =>
      makeMemory({ id: `p2-${i}`, content: `Page2 mem ${i}` }),
    );
    const listSpy = vi.spyOn(api.memories, 'list').mockImplementation(async (params?: Record<string, string>) => {
      const offset = Number(params?.offset ?? '0');
      return {
        total: 200,
        memories: offset === 0 ? page1 : page2,
      };
    });

    renderPage();
    await screen.findByText('Page1 mem 0');

    const nav = await screen.findByRole('navigation', { name: /pagination|paginacja/i });
    const previousBtn = within(nav).getByRole('button', { name: /previous|poprzedni/i });
    const nextBtn = within(nav).getByRole('button', { name: /next|następn/i });

    expect(previousBtn).toBeDisabled();
    expect(nextBtn).toBeEnabled();
    expect(nav.textContent ?? '').toMatch(/1\s*[–-]\s*100.*200/);

    await user.click(nextBtn);
    await screen.findByText('Page2 mem 0');

    // Inspect the last call's offset — that's the page advance the user
    // just triggered.
    const lastCall = listSpy.mock.calls.at(-1)?.[0] as Record<string, string> | undefined;
    expect(lastCall?.offset).toBe('100');
    expect(lastCall?.limit).toBe('100');
  });

  it('hides pagination when total fits in a single page', async () => {
    renderPage();
    await screen.findByText('Alpha memory');
    expect(screen.queryByRole('navigation', { name: /pagination|paginacja/i })).not.toBeInTheDocument();
  });

  it('delete asks for confirmation and skips when the user cancels', async () => {
    const user = userEvent.setup();
    const deleteSpy = vi.spyOn(api.memories, 'delete');

    renderPage();
    await screen.findByText('Alpha memory');

    await user.click(screen.getByRole('checkbox', { name: /Alpha memory/i }));

    const toolbar = await screen.findByRole('region', { name: /bulk action toolbar/i });
    // The label includes a leading "✕" glyph; match by name fragment.
    await user.click(within(toolbar).getByRole('button', { name: /delete/i }));

    // The in-app ConfirmDialog (replaces window.confirm). Cancel it and
    // assert the destructive API never fired.
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: /cancel/i }));

    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument());
    expect(deleteSpy).not.toHaveBeenCalled();
  });
});
