import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { useDialogStore } from '@/stores/dialogs';
import { ToastProvider } from '@/stores/toast';
import { CommandPalette } from './CommandPalette';

// <dialog>.showModal/close polyfilled globally in src/test/setup.ts.

function renderPalette({ onClose = vi.fn() }: { onClose?: () => void } = {}) {
  // Fresh client per render — keeps test state isolated. Retries off so a
  // mocked rejection surfaces immediately instead of waiting for backoff.
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <ToastProvider>
            <CommandPalette open={true} onClose={onClose} />
          </ToastProvider>
        </MemoryRouter>
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

describe('CommandPalette', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('shows navigation entries by default', () => {
    renderPalette();
    expect(screen.getByRole('option', { name: /graph/i })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /memories/i })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /settings/i })).toBeInTheDocument();
  });

  it('shows action entries with a visual Action badge', () => {
    renderPalette();
    // The "Action" badge is decorative (aria-hidden=true) so it does not
    // appear in the accessible name. Verify it is still in the DOM as
    // text content so sighted users can distinguish actions from navigation.
    const dreamItem = screen.getByRole('option', { name: /run dream cycle/i });
    expect(dreamItem).toBeInTheDocument();
    expect(dreamItem.textContent).toMatch(/action/i);
  });

  it('filters entries by query', async () => {
    const user = userEvent.setup();
    renderPalette();
    const input = screen.getByRole('combobox');
    await user.type(input, 'consol');
    expect(screen.getByRole('option', { name: /run consolidation/i })).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: /graph/i })).not.toBeInTheDocument();
  });

  it('calls api.dream and shows a success toast when running the Dream action', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const dreamSpy = vi.spyOn(api, 'dream').mockImplementation(async () => ({}) as never);
    renderPalette({ onClose });

    // Click the option (semantic role), not the inner button — button itself
    // has the click handler but the option wraps it.
    const option = screen.getByRole('option', { name: /run dream cycle/i });
    const button = option.querySelector('button');
    expect(button).not.toBeNull();
    await user.click(button as HTMLButtonElement);

    await waitFor(() => expect(dreamSpy).toHaveBeenCalled(), { timeout: 3000 });
    await waitFor(() => expect(onClose).toHaveBeenCalled(), { timeout: 3000 });
    expect(await screen.findByText(/dream cycle started/i)).toBeInTheDocument();
  });

  it('shows an error toast when the action throws', async () => {
    const user = userEvent.setup();
    vi.spyOn(api, 'consolidate').mockImplementation(async () => {
      throw new Error('Backend down');
    });
    renderPalette();

    const option = screen.getByRole('option', { name: /run consolidation/i });
    const button = option.querySelector('button') as HTMLButtonElement;
    await user.click(button);

    expect(await screen.findByText(/backend down/i, undefined, { timeout: 3000 })).toBeInTheDocument();
  });

  it('exposes every routable page in the navigation block', () => {
    // The router (`App.tsx`) defines 16 page routes: graph, memories,
    // review, briefing, timeline, feed, explore, reasoning, decisions,
    // insights, hubs, intentions, temporal, stats, settings, tutorial.
    // Power users live in the palette (Cmd+K), so missing routes here
    // mean "this page is invisible to the keyboard". The original
    // palette skipped six pages (temporal, reasoning, decisions,
    // insights, hubs, tutorial) with full sidebar entries that the
    // palette pretended didn't exist.
    renderPalette();
    // Names match the EN i18n labels (nav.* in `en.json`). The palette
    // option's accessible name is the *translated* label, so we compare
    // against rendered text — not against the route path.
    const required = [
      /graph/i,
      /memories/i,
      /review/i,
      /briefing/i,
      /timeline/i,
      /activity/i, // nav.feed
      /explore/i,
      /reasoning/i,
      /decisions/i,
      /insights/i,
      /topic hubs/i,
      /reminders/i, // nav.intentions
      /temporal/i,
      /overview/i, // nav.stats
      /settings/i,
      /tutorial/i,
    ];
    for (const name of required) {
      expect(screen.getByRole('option', { name })).toBeInTheDocument();
    }
  });

  it('exposes "Add memory" as a palette action that opens the dialog store', async () => {
    // Cmd+N opens the AddMemoryDialog from anywhere on the page, but
    // power users live in Cmd+K — and the original palette had no
    // way to trigger this most-used write action. Contract: typing
    // "add" surfaces a single option whose click flips the dialog
    // store flag (Layout subscribes to it) and closes the palette.
    const user = userEvent.setup();
    const onClose = vi.fn();
    useDialogStore.setState({ addMemoryOpen: false, pendingSelectMemoryId: null });
    renderPalette({ onClose });
    const input = screen.getByRole('combobox');
    await user.type(input, 'add');

    const option = screen.getByRole('option', { name: /add memory/i });
    const button = option.querySelector('button') as HTMLButtonElement;
    await user.click(button);

    expect(useDialogStore.getState().addMemoryOpen).toBe(true);
    expect(onClose).toHaveBeenCalled();
  });

  it('navigates via ArrowDown + Enter without arrow flip past the end', async () => {
    const user = userEvent.setup();
    renderPalette();
    const input = screen.getByRole('combobox');

    // Active item starts on first navigation entry (Graph).
    expect(input).toHaveAttribute('aria-activedescendant', 'nav-graph');

    await user.keyboard('{ArrowDown}');
    expect(input).toHaveAttribute('aria-activedescendant', 'nav-memories');

    // Many ArrowDown presses should not exceed the list bounds.
    for (let i = 0; i < 50; i++) await user.keyboard('{ArrowDown}');
    const lastId = input.getAttribute('aria-activedescendant');
    // Last item is whichever action sits at the end of the COMMAND list. We
    // pin to "action-backup" because that is the deliberate end of the list;
    // if a future change reorders items, this assertion forces an audit.
    expect(lastId).toBe('action-backup');
  });
});
