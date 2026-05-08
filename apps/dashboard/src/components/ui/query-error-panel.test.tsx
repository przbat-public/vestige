import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import { axe } from 'vitest-axe';
import i18n from '@/lib/i18n';
import { QueryErrorPanel } from './query-error-panel';

function renderWithI18n(ui: React.ReactElement) {
  return render(<I18nextProvider i18n={i18n}>{ui}</I18nextProvider>);
}

describe('QueryErrorPanel', () => {
  beforeAll(async () => {
    // Make sure English bundle is loaded so t() returns real strings.
    await i18n.changeLanguage('en');
    await i18n.loadNamespaces(['translation']);
  });

  it('renders the default fetch error title', () => {
    renderWithI18n(<QueryErrorPanel />);
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent(/failed to load data/i);
  });

  it('shows the underlying error message in a collapsible details element', async () => {
    const user = userEvent.setup();
    const error = new Error('Network 500');
    renderWithI18n(<QueryErrorPanel error={error} />);

    const summary = screen.getByText(/show technical details/i);
    expect(summary).toBeInTheDocument();
    // Detail content present (closed by default per <details>):
    expect(screen.getByText('Network 500')).toBeInTheDocument();

    // Toggling the disclosure stays keyboard-accessible.
    await user.click(summary);
  });

  it('hides the details disclosure when no error is provided', () => {
    renderWithI18n(<QueryErrorPanel />);
    expect(screen.queryByText(/show technical details/i)).not.toBeInTheDocument();
  });

  it('renders a retry button only when onRetry is supplied and calls it on click', async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    renderWithI18n(<QueryErrorPanel onRetry={onRetry} />);

    const retry = screen.getByRole('button', { name: /try again/i });
    await user.click(retry);
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it('omits the retry button when onRetry is missing', () => {
    renderWithI18n(<QueryErrorPanel />);
    expect(screen.queryByRole('button', { name: /try again/i })).not.toBeInTheDocument();
  });

  it('uses a custom title when provided', () => {
    renderWithI18n(<QueryErrorPanel title="Custom error" />);
    expect(screen.getByText('Custom error')).toBeInTheDocument();
  });

  it('has no a11y violations', async () => {
    const { container } = renderWithI18n(
      <QueryErrorPanel error={new Error('boom')} onRetry={() => {}} />,
    );
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('honors Polish locale for retry and details labels', async () => {
    await act(async () => {
      await i18n.changeLanguage('pl');
    });
    renderWithI18n(<QueryErrorPanel error={new Error('Awaria sieci')} onRetry={() => {}} />);

    expect(screen.getByRole('button', { name: /spróbuj ponownie/i })).toBeInTheDocument();
    expect(screen.getByText(/pokaż szczegóły techniczne/i)).toBeInTheDocument();

    // Reset for other tests
    await act(async () => {
      await i18n.changeLanguage('en');
    });
  });
});
