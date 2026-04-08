import { cn } from '@/lib/utils';

interface LoadingSpinnerProps {
  label?: string;
  className?: string;
}

export function LoadingSpinner({ label, className }: LoadingSpinnerProps) {
  return (
    <div className={cn('flex flex-col items-center justify-center gap-3 py-8', className)} role="status">
      <div
        className="w-6 h-6 rounded-full border-2 border-muted-foreground/20 border-t-primary animate-spin motion-reduce:animate-none"
        aria-hidden="true"
      />
      {label && <span className="text-sm text-muted-foreground">{label}</span>}
      <span className="sr-only">{label || 'Loading'}</span>
    </div>
  );
}
