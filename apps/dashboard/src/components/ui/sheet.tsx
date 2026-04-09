import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const sheetVariants = cva(
  'fixed z-20 bg-card/95 backdrop-blur-xl border-border overflow-y-auto transition-transform duration-300',
  {
    variants: {
      side: {
        right: 'inset-y-0 right-0 border-l',
        left: 'inset-y-0 left-0 border-r',
      },
      size: {
        sm: 'w-80',
        md: 'w-96',
        lg: 'w-[480px]',
      },
    },
    defaultVariants: { side: 'right', size: 'md' },
  },
);

interface SheetProps extends React.HTMLAttributes<HTMLDivElement>, VariantProps<typeof sheetVariants> {
  open: boolean;
}

export function Sheet({ open, side, size, className, children, ...props }: SheetProps) {
  if (!open) return null;
  return (
    <div className={cn(sheetVariants({ side, size }), className)} {...props}>
      {children}
    </div>
  );
}
