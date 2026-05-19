import { cn } from '@/lib/utils';

interface ConfidenceRingProps {
  /** Value 0..1 (a fraction); values outside the range are clamped. */
  value: number;
  /** Pixel diameter of the ring. Defaults to 48. */
  size?: number;
  /** Stroke width of the ring track. Defaults to ~12% of size for readability. */
  strokeWidth?: number;
  /** Label rendered inside the ring. Defaults to `XX%` of `value`. */
  label?: string;
  /** Optional caption rendered next to the ring. */
  caption?: string;
  /** Forces a tone instead of deriving from value (e.g. for trust-of-evidence). */
  tone?: ConfidenceTone;
  /** Aria label for screen readers. Defaults to caption + label. */
  ariaLabel?: string;
  className?: string;
}

export type ConfidenceTone = 'success' | 'warning' | 'danger' | 'neutral';

/**
 * Compact circular progress visualization for normalized 0..1 metrics —
 * confidence, trust, retention. The ring fills clockwise from 12 o'clock
 * and is colour-keyed so a glance at the colour conveys "this is good"
 * vs "this is shaky" without reading the number.
 *
 * Tone thresholds match the dashboard's Badge conventions so a memory's
 * confidence ring and confidence badge never disagree about whether the
 * number is good news.
 */
export function ConfidenceRing({
  value,
  size = 48,
  strokeWidth,
  label,
  caption,
  tone,
  ariaLabel,
  className,
}: ConfidenceRingProps) {
  const clamped = Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0));
  const pct = Math.round(clamped * 100);
  // Default to ~12% of diameter (rounded to integer px so SVG strokes stay
  // crisp). Caller can override for dense layouts where a slim track reads
  // better.
  const sw = strokeWidth ?? Math.max(3, Math.round(size * 0.12));
  const radius = (size - sw) / 2;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference * (1 - clamped);

  const effectiveTone: ConfidenceTone = tone ?? deriveTone(clamped);
  const colorClass = TONE_STROKE[effectiveTone];

  const text = label ?? `${pct}%`;
  const aria = ariaLabel ?? (caption ? `${caption}: ${text}` : text);

  return (
    <div className={cn('inline-flex items-center gap-2', className)} role="img" aria-label={aria}>
      <div className="relative inline-block shrink-0" style={{ width: size, height: size }}>
        <svg
          width={size}
          height={size}
          viewBox={`0 0 ${size} ${size}`}
          aria-hidden="true"
          // -90° rotation so the arc starts at 12 o'clock instead of 3 — matches
          // the cultural expectation of "progress fills like a clock face".
          className="-rotate-90"
        >
          <circle cx={size / 2} cy={size / 2} r={radius} fill="none" strokeWidth={sw} className="stroke-muted/30" />
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            strokeWidth={sw}
            strokeLinecap="round"
            strokeDasharray={circumference}
            strokeDashoffset={dashOffset}
            className={cn('transition-[stroke-dashoffset] duration-500 ease-out', colorClass)}
          />
        </svg>
        <span
          className={cn(
            'absolute inset-0 inline-flex items-center justify-center',
            'tabular-nums font-semibold pointer-events-none',
            textSizeFor(size),
          )}
        >
          {text}
        </span>
      </div>
      {caption && (
        <span className="text-xs text-muted-foreground" aria-hidden="true">
          {caption}
        </span>
      )}
    </div>
  );
}

const TONE_STROKE: Record<ConfidenceTone, string> = {
  success: 'stroke-success',
  warning: 'stroke-warning',
  danger: 'stroke-danger',
  neutral: 'stroke-primary',
};

function deriveTone(value: number): ConfidenceTone {
  if (value >= 0.7) return 'success';
  if (value >= 0.4) return 'warning';
  return 'danger';
}

function textSizeFor(size: number): string {
  if (size >= 64) return 'text-base';
  if (size >= 48) return 'text-xs';
  return 'text-[10px]';
}
