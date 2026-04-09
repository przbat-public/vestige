import { cn } from '@/lib/utils';

interface ProgressBarProps {
  value: number;
  max?: number;
  label: string;
  color?: string;
  showValue?: boolean;
  className?: string;
}

export function ProgressBar({ value, max = 100, label, color, showValue = true, className }: ProgressBarProps) {
  const pct = Math.min(100, Math.max(0, (value / max) * 100));

  return (
    <div className={cn('flex items-center gap-2', className)}>
      <div
        className="flex-1 h-1.5 rounded-full bg-muted/30 overflow-hidden"
        role="progressbar"
        aria-valuenow={Math.round(value)}
        aria-valuemin={0}
        aria-valuemax={max}
        aria-label={label}
      >
        <div
          className="h-full rounded-full transition-all duration-500"
          style={{
            width: `${pct}%`,
            backgroundColor: color || 'var(--color-primary)',
          }}
        />
      </div>
      {showValue && (
        <span className="text-xs tabular-nums text-muted-foreground min-w-[3ch] text-right">{Math.round(pct)}%</span>
      )}
    </div>
  );
}
