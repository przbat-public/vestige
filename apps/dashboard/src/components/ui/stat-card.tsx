import { cn } from '@/lib/utils';
import { Card } from './card';

interface StatCardProps {
  label: string;
  value: string | number;
  detail?: string;
  color?: string;
  className?: string;
}

export function StatCard({ label, value, detail, color, className }: StatCardProps) {
  return (
    <Card className={cn('flex flex-col gap-1', className)}>
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-lg font-semibold tabular-nums" style={color ? { color } : undefined}>
        {value}
      </span>
      {detail && <span className="text-xs text-muted-foreground">{detail}</span>}
    </Card>
  );
}
