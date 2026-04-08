import { type VariantProps, cva } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const badgeVariants = cva(
  'inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium transition-colors',
  {
    variants: {
      variant: {
        default: 'bg-primary/10 text-primary border border-primary/20',
        secondary: 'bg-muted text-muted-foreground',
        outline: 'border text-foreground',
        success: 'bg-emerald-500/10 text-emerald-500 border border-emerald-500/20',
        warning: 'bg-amber-500/10 text-amber-500 border border-amber-500/20',
        danger: 'bg-red-500/10 text-red-500 border border-red-500/20',
      },
    },
    defaultVariants: { variant: 'default' },
  },
);

interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badgeVariants> {
  color?: string;
}

export function Badge({ className, variant, color, style, ...props }: BadgeProps) {
  const colorStyle = color
    ? {
        color,
        borderColor: `${color}40`,
        backgroundColor: `${color}15`,
        ...style,
      }
    : style;

  return <span className={cn(badgeVariants({ variant }), color && 'border', className)} style={colorStyle} {...props} />;
}
