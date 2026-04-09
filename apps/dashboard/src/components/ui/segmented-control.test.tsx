import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { SegmentedControl } from './segmented-control';

const noop = () => undefined;

const OPTIONS = [
  { value: 'a', label: 'Alpha' },
  { value: 'b', label: 'Beta' },
  { value: 'c', label: 'Gamma' },
] as const;

describe('SegmentedControl', () => {
  it('renders all options', () => {
    render(<SegmentedControl value="a" onChange={noop} options={OPTIONS} />);
    expect(screen.getByText('Alpha')).toBeInTheDocument();
    expect(screen.getByText('Beta')).toBeInTheDocument();
    expect(screen.getByText('Gamma')).toBeInTheDocument();
  });

  it('marks active option as pressed', () => {
    render(<SegmentedControl value="b" onChange={noop} options={OPTIONS} />);
    expect(screen.getByText('Beta')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('Alpha')).toHaveAttribute('aria-pressed', 'false');
  });

  it('calls onChange with the selected value', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<SegmentedControl value="a" onChange={onChange} options={OPTIONS} />);
    await user.click(screen.getByText('Gamma'));
    expect(onChange).toHaveBeenCalledWith('c');
  });

  it('has no a11y violations', async () => {
    const { container } = render(<SegmentedControl value="a" onChange={noop} options={OPTIONS} />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
