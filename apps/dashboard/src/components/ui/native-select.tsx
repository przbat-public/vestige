import { cn } from '@/lib/utils';

interface NativeSelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  ref?: React.Ref<HTMLSelectElement>;
}

export function NativeSelect({ className, ref, ...props }: NativeSelectProps) {
  return (
    <select
      ref={ref}
      className={cn(
        'px-2 py-2 rounded-lg text-sm bg-background border border-border text-foreground focus:outline-none focus:ring-2 focus:ring-ring transition',
        className,
      )}
      {...props}
    />
  );
}
