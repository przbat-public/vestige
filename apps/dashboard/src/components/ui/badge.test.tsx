import { render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { Badge } from './badge';

describe('Badge', () => {
  it('renders text content', () => {
    render(<Badge>Status</Badge>);
    expect(screen.getByText('Status')).toBeInTheDocument();
  });

  it('applies variant classes', () => {
    render(<Badge variant="success">OK</Badge>);
    expect(screen.getByText('OK')).toHaveClass('bg-emerald-500/10');
  });

  it('applies custom color style', () => {
    render(<Badge color="#ff0000">Custom</Badge>);
    const el = screen.getByText('Custom');
    expect(el).toHaveStyle({ color: '#ff0000' });
  });

  it('has no a11y violations', async () => {
    const { container } = render(<Badge>Label</Badge>);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
