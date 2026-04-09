import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const alertVariants = cva('rounded-xl border p-4 text-sm', {
  variants: {
    variant: {
      default: 'bg-card border-border text-foreground',
      destructive: 'bg-red-500/5 border-red-500/30 text-red-600 dark:text-red-400',
      warning: 'bg-amber-500/5 border-amber-500/30 text-amber-600 dark:text-amber-400',
      success: 'bg-emerald-500/5 border-emerald-500/30 text-emerald-600 dark:text-emerald-400',
    },
  },
  defaultVariants: { variant: 'default' },
});

interface AlertProps extends React.HTMLAttributes<HTMLDivElement>, VariantProps<typeof alertVariants> {}

export function Alert({ className, variant, ...props }: AlertProps) {
  return <div role="alert" className={cn(alertVariants({ variant }), className)} {...props} />;
}
