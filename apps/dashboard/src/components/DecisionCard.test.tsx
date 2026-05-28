import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { MemoryRouter } from 'react-router';
import i18n from '@/lib/i18n';
import { useDialogStore } from '@/stores/dialogs';
import type { Decision } from '@/types';
import { DecisionCard } from './DecisionCard';

const mockNavigate = vi.fn();
vi.mock('react-router', async () => {
  const actual = (await vi.importActual<typeof import('react-router')>('react-router')) as Record<string, unknown>;
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

function makeDecision(overrides: Partial<Decision> = {}): Decision {
  return {
    id: 'decision-mem-123',
    question: 'Postgres vs MySQL for the vector store?',
    rationale: 'Postgres has pgvector and we already operate it.',
    choices: [
      { id: 'pg', label: 'Postgres', chosen: true, summary: 'native pgvector' },
      { id: 'my', label: 'MySQL', chosen: false, summary: 'no vector ops' },
    ],
    criteria: [
      { id: 'ops', label: 'Operational fit', weight: 0.6 },
      { id: 'perf', label: 'Vector perf', weight: 0.4 },
    ],
    scoreMatrix: [
      { criterionId: 'ops', choiceId: 'pg', score: 4.5 },
      { criterionId: 'ops', choiceId: 'my', score: 2.0 },
      { criterionId: 'perf', choiceId: 'pg', score: 4.0 },
    ],
    supersedes: [],
    createdAt: '2026-05-12T10:00:00Z',
    validUntil: null,
    expired: false,
    tags: ['db', 'decision'],
    ...overrides,
  } as Decision;
}

describe('DecisionCard', () => {
  beforeEach(() => {
    mockNavigate.mockReset();
    useDialogStore.setState({ pendingSelectMemoryId: null });
  });

  it('renders the decision question, the recommended choice, and the score matrix', () => {
    render(
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <DecisionCard decision={makeDecision()} />
        </MemoryRouter>
      </I18nextProvider>,
    );
    // The question lives in two DOM nodes (visible heading + sr-only
    // table caption for screen readers). `getAllByText` is the
    // intentional shape here — asserting both renders means a future
    // refactor that drops the accessible caption regresses loudly.
    expect(screen.getAllByText(/Postgres vs MySQL/i).length).toBeGreaterThanOrEqual(2);
    // Score cells render with one decimal — 4.5 must appear literally,
    // not as 5 or "4". This is the contract DecisionCard documents.
    expect(screen.getByText('4.5')).toBeInTheDocument();
    // Sparse cell (perf × MySQL is missing) renders as an em-dash, not "0".
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
  });

  it('exposes the question as an interactive control that opens the host memory drawer', async () => {
    // Regression target: decisions are first-class memories — the host
    // node carries the full memory payload (created_at, tags,
    // retention, edit history). Without a click-through the user could
    // see the comparison matrix but had no way to inspect the raw
    // memory or its connections, so the page felt like a dead end.
    const user = userEvent.setup();
    render(
      <I18nextProvider i18n={i18n}>
        <MemoryRouter>
          <DecisionCard decision={makeDecision()} />
        </MemoryRouter>
      </I18nextProvider>,
    );

    const trigger = screen.getByRole('button', { name: /Postgres vs MySQL/i });
    await user.click(trigger);

    expect(useDialogStore.getState().pendingSelectMemoryId).toBe('decision-mem-123');
    expect(mockNavigate).toHaveBeenCalledWith('/memories');
  });
});
