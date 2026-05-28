import { fireEvent, render, screen } from '@testing-library/react';
import { GraphHelpOverlay } from './GraphHelpOverlay';

// We don't mock i18next — the dashboard test setup already wires the real
// instance with the English bundle, so missing keys would surface here
// before they ever reach a user.

describe('GraphHelpOverlay', () => {
  it('returns nothing when closed', () => {
    const { container } = render(<GraphHelpOverlay open={false} onClose={vi.fn()} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders as an aria-modal dialog with a heading when open', () => {
    render(<GraphHelpOverlay open={true} onClose={vi.fn()} />);
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    // Heading is what AT will announce when the dialog opens.
    expect(screen.getByRole('heading', { level: 2 })).toBeInTheDocument();
  });

  it('lists the documented shortcut keys', () => {
    render(<GraphHelpOverlay open={true} onClose={vi.fn()} />);
    // Spot-check key entries — we don't assert the entire matrix because
    // copy/translations evolve, but these are the entries the user should
    // always see: navigation, framing, history, help itself.
    expect(screen.getByText('F')).toBeInTheDocument();
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('R')).toBeInTheDocument();
    expect(screen.getByText('?')).toBeInTheDocument();
    expect(screen.getByText('Esc')).toBeInTheDocument();
  });

  it('calls onClose when Escape is pressed', () => {
    const onClose = vi.fn();
    render(<GraphHelpOverlay open={true} onClose={onClose} />);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not call onClose on Escape when already closed (no stale listener)', () => {
    const onClose = vi.fn();
    const { rerender } = render(<GraphHelpOverlay open={true} onClose={onClose} />);
    rerender(<GraphHelpOverlay open={false} onClose={onClose} />);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });

  it('calls onClose when the scrim is clicked', () => {
    const onClose = vi.fn();
    render(<GraphHelpOverlay open={true} onClose={onClose} />);
    const scrim = screen.getByTestId('graph-help-scrim');
    fireEvent.click(scrim);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clicking the dialog body does NOT close the overlay', () => {
    const onClose = vi.fn();
    render(<GraphHelpOverlay open={true} onClose={onClose} />);
    fireEvent.click(screen.getByRole('dialog'));
    expect(onClose).not.toHaveBeenCalled();
  });

  it('Close button triggers onClose', () => {
    const onClose = vi.fn();
    render(<GraphHelpOverlay open={true} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: /close/i }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
