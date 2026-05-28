import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { api } from '@/stores/api';
import { ToastProvider } from '@/stores/toast';
import type { GraphResponse } from '@/types';
import { GraphPage } from './GraphPage';

// Three.js/WebGL never works in JSDOM, so we substitute Graph3D with a
// thin DOM stand-in. The point of these tests is GraphPage's filter and
// empty-state branching, not the renderer.
vi.mock('@/components/Graph3D', () => ({
  Graph3D: ({ nodes }: { nodes: { id: string }[] }) => (
    <div data-testid="graph3d-mock" data-node-count={nodes.length} />
  ),
}));
vi.mock('@/components/TimeSlider', () => ({ TimeSlider: () => <div data-testid="time-slider-mock" /> }));
vi.mock('@/components/TagLegend', () => ({ TagLegend: () => null }));
vi.mock('@/components/memories/MemoryDetail', () => ({ MemoryDetail: () => <div /> }));

vi.mock('@/stores/websocket', () => ({
  useWebSocket: (selector: (s: { events: unknown[]; isDreaming: boolean }) => unknown) =>
    selector({ events: [], isDreaming: false }),
}));
vi.mock('@/stores/telemetry', () => ({
  useTrackPageView: () => undefined,
  track: () => undefined,
  EVENT: {},
}));
vi.mock('@/hooks/use-reduced-motion', () => ({ useReducedMotion: () => false }));
vi.mock('@/hooks/use-dashboard-limits', () => ({
  useDashboardLimits: () => ({
    graphDepthMax: 3,
    graphMaxNodesMax: 1000,
    insightsLimitMax: 100,
    timelineLimitMax: 1000,
    intentionLimitMax: 100,
    feedBufferMax: 100,
    explorationDepthMax: 3,
    explorationLimitMax: 100,
  }),
}));

function makeGraph(tags: string[][]): GraphResponse {
  return {
    nodes: tags.map((tagList, i) => ({
      id: `n${i}`,
      label: `Node ${i}`,
      type: 'fact',
      retention: 0.7,
      tags: tagList,
      createdAt: '2026-04-01T00:00:00Z',
      updatedAt: '2026-04-01T00:00:00Z',
      isCenter: i === 0,
    })),
    edges: [],
    centerId: 'n0',
    depth: 2,
    nodeCount: tags.length,
    edgeCount: 0,
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
            <GraphPage />
          </ToastProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe('GraphPage tag filter', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders an in-place empty state when the tag filter matches no nodes', async () => {
    // Pre-fix: typing a tag that matched nothing left the user staring
    // at a blank 3D canvas with no explanation. The graph hadn't broken
    // — the filter just hid every node. We now surface that distinction.
    await i18n.changeLanguage('en');
    vi.spyOn(api, 'graph').mockResolvedValue(makeGraph([['rust'], ['python']]));

    renderPage();
    // Wait for the graph to load — the mock renders with 2 nodes.
    await waitFor(() => {
      expect(screen.getByTestId('graph3d-mock')).toHaveAttribute('data-node-count', '2');
    });

    const user = userEvent.setup();
    const tagInput = screen.getByLabelText(/filter by tag/i);
    await user.type(tagInput, 'haskell');

    // No-match panel appears, and the canvas is told the graph is now empty.
    expect(await screen.findByText(/No nodes match the current filter/i)).toBeInTheDocument();
    expect(screen.getByTestId('graph3d-mock')).toHaveAttribute('data-node-count', '0');
  });

  it('clears the filter and brings the graph back when the user clicks "Clear filter"', async () => {
    await i18n.changeLanguage('en');
    vi.spyOn(api, 'graph').mockResolvedValue(makeGraph([['rust'], ['python']]));

    renderPage();
    await waitFor(() => {
      expect(screen.getByTestId('graph3d-mock')).toHaveAttribute('data-node-count', '2');
    });

    const user = userEvent.setup();
    const tagInput = screen.getByLabelText(/filter by tag/i);
    await user.type(tagInput, 'haskell');
    expect(await screen.findByText(/No nodes match the current filter/i)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /clear filter/i }));

    expect(screen.queryByText(/No nodes match the current filter/i)).not.toBeInTheDocument();
    expect(screen.getByTestId('graph3d-mock')).toHaveAttribute('data-node-count', '2');
    expect((tagInput as HTMLInputElement).value).toBe('');
  });

  it('does NOT show the empty state when no tag filter is set (initial render)', async () => {
    await i18n.changeLanguage('en');
    vi.spyOn(api, 'graph').mockResolvedValue(makeGraph([['rust'], ['python']]));

    renderPage();
    await waitFor(() => {
      expect(screen.getByTestId('graph3d-mock')).toBeInTheDocument();
    });
    expect(screen.queryByText(/No nodes match the current filter/i)).not.toBeInTheDocument();
  });
});
