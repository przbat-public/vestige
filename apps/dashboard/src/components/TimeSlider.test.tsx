import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/lib/i18n';
import type { GraphNode } from '@/types';
import { TimeSlider } from './TimeSlider';

function makeNode(id: string, createdAt: string): GraphNode {
  return {
    id,
    label: id,
    type: 'fact',
    retention: 0.7,
    tags: [],
    createdAt,
    updatedAt: createdAt,
    isCenter: false,
  };
}

function renderSlider() {
  const nodes = [makeNode('a', '2026-01-15T00:00:00Z'), makeNode('b', '2026-03-01T00:00:00Z')];
  return render(
    <I18nextProvider i18n={i18n}>
      <TimeSlider nodes={nodes} onDateChange={() => undefined} onToggle={() => undefined} />
    </I18nextProvider>,
  );
}

describe('TimeSlider accessibility', () => {
  it('exposes a slider role with min, max, value, and human-readable value text', async () => {
    const user = userEvent.setup();
    renderSlider();
    // The collapsed pill button opens the slider panel.
    await user.click(screen.getByRole('button', { name: /timeline/i }));

    const slider = screen.getByRole('slider');
    expect(slider).toHaveAttribute('aria-valuemin', '0');
    expect(slider).toHaveAttribute('aria-valuemax', '100');
    // aria-valuenow tracks the numeric range value...
    expect(slider).toHaveAttribute('aria-valuenow');
    // ...and aria-valuetext gives the screen reader a human-readable date
    // (NVDA/VoiceOver prefer valuetext over valuenow when both are set).
    expect(slider).toHaveAttribute('aria-valuetext');
    const valueText = slider.getAttribute('aria-valuetext');
    expect(valueText).toMatch(/\d{4}/); // contains a year
  });
});

describe('TimeSlider play/resume', () => {
  it('play from the end restarts at zero (the only sensible interpretation)', async () => {
    const user = userEvent.setup();
    renderSlider();
    await user.click(screen.getByRole('button', { name: /timeline/i }));
    // After enabling, the slider sits at 100% (full range visible).
    const slider = screen.getByRole('slider');
    expect(slider).toHaveAttribute('aria-valuenow', '100');
    await user.click(screen.getByRole('button', { name: /play/i }));
    // At end-of-range the only place play can go is back to the start.
    expect(slider).toHaveAttribute('aria-valuenow', '0');
  });

  it('play resumes from the current slider position when not at the end', async () => {
    // Regression for the "play always jumped to 0" UX bug: the user
    // scrubs the timeline to inspect a specific point in history, then
    // hits play to watch the rest of the period unfold. Without this
    // guard the click would teleport the cursor back to the oldest
    // memory and discard the inspection.
    const user = userEvent.setup();
    renderSlider();
    await user.click(screen.getByRole('button', { name: /timeline/i }));
    const slider = screen.getByRole('slider');
    fireEvent.change(slider, { target: { value: '42' } });
    expect(slider).toHaveAttribute('aria-valuenow', '42');
    await user.click(screen.getByRole('button', { name: /play/i }));
    // requestAnimationFrame in JSDOM is async so the value at this
    // synchronous tick is still whatever the click handler set.
    expect(Number(slider.getAttribute('aria-valuenow'))).toBeGreaterThanOrEqual(40);
    expect(Number(slider.getAttribute('aria-valuenow'))).toBeLessThanOrEqual(50);
  });
});
