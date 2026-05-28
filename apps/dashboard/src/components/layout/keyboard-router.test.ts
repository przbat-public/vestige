import { isInsideApplicationWidget, isTypingInEditable } from './keyboard-router';

describe('isTypingInEditable', () => {
  it('returns true for an <input>', () => {
    const input = document.createElement('input');
    expect(isTypingInEditable(input)).toBe(true);
  });
  it('returns true for a <textarea>', () => {
    const ta = document.createElement('textarea');
    expect(isTypingInEditable(ta)).toBe(true);
  });
  it('returns true for a contentEditable element', () => {
    // JSDOM does not compute `isContentEditable` from the attribute, so
    // we fake the getter on a real HTMLElement to mirror what a real
    // browser exposes.
    const div = document.createElement('div');
    Object.defineProperty(div, 'isContentEditable', { value: true, configurable: true });
    expect(isTypingInEditable(div)).toBe(true);
  });
  it('returns false for a plain <div>', () => {
    expect(isTypingInEditable(document.createElement('div'))).toBe(false);
  });
  it('returns false for null', () => {
    expect(isTypingInEditable(null)).toBe(false);
  });
});

describe('isInsideApplicationWidget', () => {
  it('returns true when the target itself has role="application"', () => {
    const app = document.createElement('div');
    app.setAttribute('role', 'application');
    expect(isInsideApplicationWidget(app)).toBe(true);
  });

  it('returns true when an ancestor carries role="application"', () => {
    const app = document.createElement('div');
    app.setAttribute('role', 'application');
    const child = document.createElement('span');
    app.appendChild(child);
    expect(isInsideApplicationWidget(child)).toBe(true);
  });

  it('returns false for an unrelated focused element', () => {
    const wrapper = document.createElement('div');
    const inside = document.createElement('span');
    wrapper.appendChild(inside);
    expect(isInsideApplicationWidget(inside)).toBe(false);
  });

  // Regression for the Graph page double-dialog bug: when focus is
  // inside Graph3D's `role="application"` container and the user
  // presses `?`, the global router must yield to the canvas's own
  // help overlay handler. Without this guard, both opened at once.
  it('lets Graph3D own its `?` shortcut by reporting true inside the canvas', () => {
    const canvas = document.createElement('div');
    canvas.setAttribute('role', 'application');
    canvas.tabIndex = 0;
    const ul = document.createElement('ul');
    canvas.appendChild(ul);
    expect(isInsideApplicationWidget(canvas)).toBe(true);
    expect(isInsideApplicationWidget(ul)).toBe(true);
  });

  it('returns false for null / non-Element targets', () => {
    expect(isInsideApplicationWidget(null)).toBe(false);
    expect(isInsideApplicationWidget({ tagName: 'DIV' } as unknown as EventTarget)).toBe(false);
  });
});
