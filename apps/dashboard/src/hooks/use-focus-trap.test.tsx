import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useRef, useState } from 'react';
import { useFocusTrap } from './use-focus-trap';

/**
 * The focus trap exists because `aria-modal="true"` promises assistive
 * technology that the rest of the document is inert. For dialogs built on
 * `<div>` that promise has to be kept by hand — otherwise Tab walks out
 * behind the scrim (WCAG 2.4.3).
 *
 * jsdom does not implement layout, so `getFocusable`'s visibility check
 * (`offsetParent`) would see every element as hidden. Stub it out: these
 * tests are about the tab cycle, not about CSS.
 */

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'offsetParent', {
    configurable: true,
    get() {
      return this.parentElement ?? document.body;
    },
  });
});

afterEach(() => {
  Reflect.deleteProperty(HTMLElement.prototype, 'offsetParent');
});

function Harness({ open = true, preferred = false }: { open?: boolean; preferred?: boolean }) {
  const preferredRef = useRef<HTMLButtonElement>(null);
  const containerRef = useFocusTrap<HTMLDivElement>({
    active: open,
    initialFocus: preferred ? preferredRef : undefined,
  });
  return (
    <div ref={containerRef} tabIndex={-1} data-testid="trap">
      <button type="button" ref={preferredRef}>
        preferred
      </button>
      <button type="button">first</button>
      <button type="button">last</button>
    </div>
  );
}

/** Opens the trap after `opener` already holds focus, like a real dialog. */
function InteractiveHarness() {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button type="button" onClick={() => setOpen(true)}>
        opener
      </button>
      {open && (
        <div>
          <Harness />
          <button type="button" onClick={() => setOpen(false)}>
            close
          </button>
        </div>
      )}
    </div>
  );
}

describe('useFocusTrap', () => {
  it('moves focus into the dialog on open', () => {
    render(<Harness />);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'preferred' }));
  });

  it('honours an explicit initialFocus target', () => {
    render(<Harness preferred />);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'preferred' }));
  });

  it('wraps Tab from the last element back to the first', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const last = screen.getByRole('button', { name: 'last' });
    last.focus();

    await user.tab();

    // DOM order inside the dialog: preferred → first → last.
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'preferred' }));
  });

  it('wraps Shift+Tab from the first element to the last', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const first = screen.getByRole('button', { name: 'preferred' });
    first.focus();

    await user.tab({ shift: true });

    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'last' }));
  });

  it('pulls focus back in when it has escaped the dialog', async () => {
    const user = userEvent.setup();
    render(
      <div>
        <button type="button">outside</button>
        <Harness />
      </div>,
    );
    screen.getByRole('button', { name: 'outside' }).focus();

    await user.tab();

    expect(screen.getByTestId('trap').contains(document.activeElement)).toBe(true);
  });

  it('returns focus to the opener when the dialog closes', async () => {
    const user = userEvent.setup();
    render(<InteractiveHarness />);
    const opener = screen.getByRole('button', { name: 'opener' });

    await user.click(opener);
    expect(screen.getByTestId('trap').contains(document.activeElement)).toBe(true);

    await user.click(screen.getByRole('button', { name: 'close' }));

    expect(document.activeElement).toBe(opener);
  });
});
