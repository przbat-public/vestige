import { forwardRef } from 'react';
import { cn } from '@/lib/utils';

export const NativeSelect = forwardRef<HTMLSelectElement, React.SelectHTMLAttributes<HTMLSelectElement>>(
  ({ className, ...props }, ref) => (
    <select
      ref={ref}
      className={cn(
        'px-2 py-2 rounded-lg text-sm bg-background border border-border text-foreground focus:outline-none focus:ring-2 focus:ring-ring transition',
        className,
      )}
      {...props}
    />
  ),
);

NativeSelect.displayName = 'NativeSelect';
