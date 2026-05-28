import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { ToastProvider } from '@/stores/toast';
import { useWebSocket } from '@/stores/websocket';
import type { IdentifiedEvent } from '@/types';
import { FeedPage } from './FeedPage';

function memoryCreated(id: number, ts: string, idValue = `${id}`): IdentifiedEvent {
  return {
    type: 'MemoryCreated',
    data: {
      id: idValue,
      contentPreview: `content ${id}`,
      nodeType: 'fact',
      tags: [],
      timestamp: ts,
    },
    _id: id,
  } as IdentifiedEvent;
}

function dreamStarted(id: number, ts: string): IdentifiedEvent {
  return {
    type: 'DreamStarted',
    data: { memoryCount: 50, timestamp: ts },
    _id: id,
  } as IdentifiedEvent;
}

function setStoreEvents(events: IdentifiedEvent[]) {
  useWebSocket.setState({ events });
}

function renderPage() {
  return render(
    <I18nextProvider i18n={i18n}>
      <MemoryRouter>
        <ToastProvider>
          <FeedPage />
        </ToastProvider>
      </MemoryRouter>
    </I18nextProvider>,
  );
}

describe('FeedPage', () => {
  beforeEach(() => {
    useWebSocket.setState({ events: [] });
  });

  it('renders the hh:mm:ss timestamp of each event next to its type', () => {
    setStoreEvents([memoryCreated(1, '2026-05-22T12:34:56.000Z')]);
    renderPage();

    // The component formats with `toLocaleTimeString`. Time zone is the test
    // env's default (typically UTC). The test must therefore assert on the
    // structural shape rather than a fixed literal — pattern "HH:MM:SS".
    expect(screen.getByText(/^\d{1,2}:\d{2}:\d{2}/)).toBeInTheDocument();
  });

  it('filters events by type when the user picks one from the dropdown', async () => {
    setStoreEvents([memoryCreated(1, '2026-05-22T12:00:00Z'), dreamStarted(2, '2026-05-22T12:01:00Z')]);
    renderPage();

    expect(screen.getAllByTestId('feed-event')).toHaveLength(2);

    const select = screen.getByRole('combobox');
    await userEvent.selectOptions(select, 'DreamStarted');

    const rows = screen.getAllByTestId('feed-event');
    expect(rows).toHaveLength(1);
    expect(within(rows[0]).getByText('DreamStarted')).toBeInTheDocument();
  });

  it('freezes the visible list while paused and resumes after unpausing', async () => {
    setStoreEvents([memoryCreated(1, '2026-05-22T12:00:00Z')]);
    renderPage();

    expect(screen.getAllByTestId('feed-event')).toHaveLength(1);

    const pauseBtn = screen.getByRole('button', { name: /pause|wstrzymaj/i });
    await userEvent.click(pauseBtn);

    // Now flood the store with a second event. Paused mode must hide it.
    act(() => {
      setStoreEvents([memoryCreated(2, '2026-05-22T12:05:00Z', '2'), memoryCreated(1, '2026-05-22T12:00:00Z')]);
    });
    // Still just the original one rendered (snapshot frozen at pause time).
    expect(screen.getAllByTestId('feed-event')).toHaveLength(1);

    const resumeBtn = screen.getByRole('button', { name: /resume|wznów|live|na żywo/i });
    await userEvent.click(resumeBtn);
    expect(screen.getAllByTestId('feed-event')).toHaveLength(2);
  });

  it('keeps the JSON preview block to a bounded length so wide payloads do not break layout', () => {
    const huge = 'x'.repeat(10_000);
    setStoreEvents([
      {
        type: 'MemoryCreated',
        data: {
          id: 'a',
          contentPreview: huge,
          nodeType: 'fact',
          tags: [],
          timestamp: '2026-05-22T12:00:00Z',
        },
        _id: 1,
      } as IdentifiedEvent,
    ]);
    renderPage();

    const row = screen.getByTestId('feed-event');
    const pre = within(row).getByText(/contentPreview/);
    // Hard cap: was 200, now 220 (so timestamp+nodeType still survive the slice).
    expect(pre.textContent?.length ?? 0).toBeLessThanOrEqual(220);
  });
});
