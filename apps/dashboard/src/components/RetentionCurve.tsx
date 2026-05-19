import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { retentionColor } from '@/types';

interface Props {
  retention: number;
  stability: number;
  width?: number;
  height?: number;
}

// FSRS-6 power-law forgetting curve, matched to the tutorial sandbox
// (`RetentionCurveWidget`) so the two visualisations on the dashboard never
// disagree.
//
// Formula:  R(t) = (1 + FACTOR * t / S) ^ DECAY
// with `FACTOR = 19 / 81` and `DECAY = -0.5`, the canonical FSRS-6 constants
// for `w20 = 0.5`.
//
// The previous implementation used a plain exponential `exp(-t/S)`. That is
// the Ebbinghaus curve, not FSRS-6, and it was visibly inconsistent with
// the rest of the engine (which is power-law). Replaced 2026-05-19.
const FSRS_FACTOR = 19 / 81;
const FSRS_DECAY = -0.5;

function retentionAtDays(days: number, stability: number): number {
  if (stability <= 0) return 0;
  if (days <= 0) return 1;
  return (1 + (FSRS_FACTOR * days) / stability) ** FSRS_DECAY;
}

export function RetentionCurve({ retention, stability, width = 240, height = 80 }: Props) {
  const { t } = useTranslation();

  const curvePath = useMemo(() => {
    const points: string[] = [];
    const maxDays = Math.max(stability * 3, 30);
    const padding = 4;
    const w = width - padding * 2;
    const h = height - padding * 2;
    for (let i = 0; i <= 50; i++) {
      const tp = (i / 50) * maxDays;
      const r = retentionAtDays(tp, stability);
      const x = padding + (i / 50) * w;
      const y = padding + (1 - r) * h;
      points.push(`${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`);
    }
    return points.join(' ');
  }, [stability, width, height]);

  const predictions = [
    { labelKey: 'retention.now', days: 0, value: retention },
    { labelKey: 'retention.1d', days: 1, value: retentionAtDays(1, stability) },
    { labelKey: 'retention.7d', days: 7, value: retentionAtDays(7, stability) },
    { labelKey: 'retention.30d', days: 30, value: retentionAtDays(30, stability) },
  ];

  const gradientId = `curveGrad-${width}-${height}`;

  return (
    <div className="space-y-2">
      <svg width={width} height={height} className="w-full" viewBox={`0 0 ${width} ${height}`}>
        <title>{t('retention.curveTitle')}</title>
        <line
          x1="4"
          y1={4 + (height - 8) * 0.5}
          x2={width - 4}
          y2={4 + (height - 8) * 0.5}
          stroke="#2a2a5e"
          strokeWidth="0.5"
          strokeDasharray="2,4"
        />
        <line
          x1="4"
          y1={4 + (height - 8) * 0.8}
          x2={width - 4}
          y2={4 + (height - 8) * 0.8}
          stroke="#ef444430"
          strokeWidth="0.5"
          strokeDasharray="2,4"
        />
        <path d={curvePath} fill="none" stroke="#6366f1" strokeWidth="2" strokeLinecap="round" />
        <path
          d={`${curvePath} L${width - 4},${height - 4} L4,${height - 4} Z`}
          fill={`url(#${gradientId})`}
          opacity="0.15"
        />
        <circle cx="4" cy={4 + (1 - retention) * (height - 8)} r="3" fill={retentionColor(retention)} />
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#6366f1" />
            <stop offset="100%" stopColor="#6366f100" />
          </linearGradient>
        </defs>
      </svg>
      <div className="flex gap-2 flex-wrap">
        {predictions.map((pred) => (
          <div key={pred.labelKey} className="flex items-center gap-1 text-xs">
            <span className="text-muted-foreground">{t(pred.labelKey)}:</span>
            <span style={{ color: retentionColor(pred.value) }}>{(pred.value * 100).toFixed(0)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}
