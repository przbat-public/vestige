import { forwardRef } from 'react';
import { cn } from '@/lib/utils';

interface SearchInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  onSubmit?: () => void;
}

export const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(
  ({ className, onSubmit, ...props }, ref) => (
    <input
      ref={ref}
      type="text"
      className={cn(
        'w-full px-4 py-2.5 rounded-xl text-sm bg-background border border-border',
        'text-foreground placeholder:text-muted-foreground',
        'focus:outline-none focus:ring-2 focus:ring-ring focus:border-transparent',
        'transition-shadow',
        className,
      )}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && onSubmit) onSubmit();
      }}
      {...props}
    />
  ),
);

SearchInput.displayName = 'SearchInput';
