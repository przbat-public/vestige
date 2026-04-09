import { render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { Alert } from './alert';

describe('Alert', () => {
  it('renders with alert role', () => {
    render(<Alert>Something happened</Alert>);
    expect(screen.getByRole('alert')).toHaveTextContent('Something happened');
  });

  it('applies destructive variant', () => {
    render(<Alert variant="destructive">Error</Alert>);
    expect(screen.getByRole('alert')).toHaveClass('border-red-500/30');
  });

  it('has no a11y violations', async () => {
    const { container } = render(<Alert>Info</Alert>);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
