import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { useDialogStore } from '@/stores/dialogs';
import type { Hub } from '@/types';
import { HubCard } from './HubCard';

const mockNavigate = vi.fn();
vi.mock('react-router', async () => {
  const actual = (await vi.importActual<typeof import('react-router')>('react-router')) as Record<string, unknown>;
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

function makeHub(overrides: Partial<Hub> = {}): Hub {
  return {
    id: 'hub-1',
    content: '[Topic Hub: vestige]\nThis cluster ...',
    clusterSignature: 'sig-1',
    childIds: ['child-aaaaaaaa-1111-2222', 'child-bbbbbbbb-3333-4444'],
    childCount: 2,
    regenerationCount: 0,
    lastRegeneratedAt: '2026-05-01T12:00:00Z',
    generationMethod: 'lda',
    dominantTags: ['vestige', 'cognitive'],
    dateRange: ['2026-04-01T00:00:00Z', '2026-05-01T00:00:00Z'],
    ...overrides,
  } as Hub;
}

describe('HubCard', () => {
  beforeEach(() => {
    mockNavigate.mockReset();
    useDialogStore.setState({ pendingSelectMemoryId: null });
  });

  it('renders the hub summary and the first 8 child ids as buttons (not raw anchor tags)', () => {
    render(
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <HubCard hub={makeHub()} />
        </MemoryRouter>
      </I18nextProvider>,
    );
    // Truncated id (first 8 chars) — child-aa
    expect(screen.getByRole('button', { name: /child-aa/i })).toBeInTheDocument();
    // Regression guard: must not be an anchor pointing to /memories/:id
    const anchors = document.querySelectorAll('a[href^="/memories/"]');
    expect(anchors.length).toBe(0);
  });

  it('clicking a child id requests the memory drawer and navigates to /memories', async () => {
    const user = userEvent.setup();
    render(
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <HubCard hub={makeHub()} />
        </MemoryRouter>
      </I18nextProvider>,
    );

    await user.click(screen.getByRole('button', { name: /child-aa/i }));

    expect(useDialogStore.getState().pendingSelectMemoryId).toBe('child-aaaaaaaa-1111-2222');
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });
});
