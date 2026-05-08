import { forwardRef, useEffect, useRef } from 'react';
import { cn } from '@/lib/utils';

interface CheckboxProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'type'> {
  /**
   * Tri-state support. When true, sets the indeterminate property on the
   * underlying input via DOM (HTML doesn't have an "indeterminate" attribute).
   * Used by bulk-select master checkboxes that show "some" vs "all" vs "none".
   */
  indeterminate?: boolean;
}

/**
 * Native <input type="checkbox"> with consistent ring + indeterminate support.
 *
 * Stays a real native checkbox so screen readers (NVDA/JAWS/VoiceOver) get
 * proper "checkbox checked / unchecked / mixed" announcements without ARIA
 * gymnastics. Visual styling matches the design system; we use accent-color
 * for the checked state which respects high-contrast mode automatically.
 */
export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(function Checkbox(
  { indeterminate = false, className, ...props },
  ref,
) {
  const innerRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (innerRef.current) innerRef.current.indeterminate = indeterminate;
  }, [indeterminate]);

  return (
    <input
      ref={(node) => {
        innerRef.current = node;
        if (typeof ref === 'function') ref(node);
        else if (ref) ref.current = node;
      }}
      type="checkbox"
      className={cn(
        'h-4 w-4 rounded border border-border bg-card accent-primary',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background',
        'disabled:opacity-50 disabled:cursor-not-allowed',
        'cursor-pointer transition',
        className,
      )}
      {...props}
    />
  );
});
