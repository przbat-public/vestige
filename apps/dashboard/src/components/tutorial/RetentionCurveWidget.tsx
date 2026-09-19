import { useId, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EVENT, track } from '@/stores/telemetry';

/**
 * Live retention-curve sandbox. The user drags two sliders — "days since
 * last review" and "stability" — and watches the FSRS-6 retention curve
 * react. Closes the abstraction gap between "memories decay" and "here's
 * what that decay actually looks like".
 *
 * The curve uses the FSRS-6 retrievability formula:
 *
 *   R(t) = (1 + FACTOR * t / S) ^ DECAY
 *
 * with `FACTOR = 19 / 81` and `DECAY = -0.5`, the canonical constants from
 * the Open Spaced Repetition project. Same formula the engine uses
 * (`crates/vestige-core/src/fsrs/`); we deliberately reimplement it here
 * in plain TS so the widget stays self-contained and won't break when the
 * backend evolves.
 */

const FSRS_FACTOR = 19 / 81;
const FSRS_DECAY = -0.5;

/** FSRS-6 retrievability at day `t` for a memory with stability `s` (days). */
function retrievability(daysElapsed: number, stability: number): number {
  if (stability <= 0) return 0;
  return (1 + (FSRS_FACTOR * daysElapsed) / stability) ** FSRS_DECAY;
}

const WIDTH = 480;
const HEIGHT = 180;
const PAD_LEFT = 36;
const PAD_RIGHT = 12;
const PAD_TOP = 16;
const PAD_BOTTOM = 28;
const MAX_DAYS = 90;
const SAMPLE_POINTS = 60;

interface CurvePoint {
  day: number;
  retention: number;
  x: number;
  y: number;
}

function buildCurve(stability: number): CurvePoint[] {
  const innerW = WIDTH - PAD_LEFT - PAD_RIGHT;
  const innerH = HEIGHT - PAD_TOP - PAD_BOTTOM;
  return Array.from({ length: SAMPLE_POINTS + 1 }, (_, i) => {
    const day = (i / SAMPLE_POINTS) * MAX_DAYS;
    const r = retrievability(day, stability);
    return {
      day,
      retention: r,
      x: PAD_LEFT + (day / MAX_DAYS) * innerW,
      y: PAD_TOP + (1 - r) * innerH,
    };
  });
}

export function RetentionCurveWidget() {
  const { t } = useTranslation();
  const [day, setDay] = useState(7);
  const [stability, setStability] = useState(14);
  const daySliderId = useId();
  const stabSliderId = useId();

  const innerW = WIDTH - PAD_LEFT - PAD_RIGHT;
  const innerH = HEIGHT - PAD_TOP - PAD_BOTTOM;
  const points = useMemo(() => buildCurve(stability), [stability]);
  const path = useMemo(
    () => points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x.toFixed(2)} ${p.y.toFixed(2)}`).join(' '),
    [points],
  );
  const currentR = retrievability(day, stability);
  const cursorX = PAD_LEFT + (day / MAX_DAYS) * innerW;
  const cursorY = PAD_TOP + (1 - currentR) * innerH;
  // Anchor x-axis ticks at meaningful intervals — daily for the first
  // week, then twice-monthly. Hard-coded because eyeballed labels read
  // better than algorithmic ones at this scale.
  const ticks = [0, 7, 14, 30, 60, 90];

  const onDay = (v: number) => {
    setDay(v);
    track(EVENT.tutorial_curve_drag, { day: v, stability });
  };
  const onStab = (v: number) => {
    setStability(v);
    track(EVENT.tutorial_curve_drag, { day, stability: v });
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('tutorial.curve.title')}</CardTitle>
        <CardDescription>{t('tutorial.curve.subtitle')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="overflow-x-auto">
          <svg
            viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
            className="w-full max-w-[480px] h-auto"
            role="img"
            aria-label={t('tutorial.curve.svgAria')}
          >
            <title>{t('tutorial.curve.title')}</title>

            {/* Horizontal gridlines at 25/50/75/100% */}
            {[0, 0.25, 0.5, 0.75, 1].map((frac) => {
              const y = PAD_TOP + (1 - frac) * innerH;
              return (
                <g key={frac}>
                  <line
                    x1={PAD_LEFT}
                    x2={WIDTH - PAD_RIGHT}
                    y1={y}
                    y2={y}
                    className="stroke-muted-foreground/20"
                    strokeWidth={1}
                  />
                  <text
                    x={PAD_LEFT - 6}
                    y={y + 3}
                    textAnchor="end"
                    className="fill-muted-foreground text-[9px] font-mono"
                  >
                    {Math.round(frac * 100)}%
                  </text>
                </g>
              );
            })}

            {/* X-axis ticks */}
            {ticks.map((tick) => {
              const x = PAD_LEFT + (tick / MAX_DAYS) * innerW;
              return (
                <g key={tick}>
                  <line
                    x1={x}
                    x2={x}
                    y1={HEIGHT - PAD_BOTTOM}
                    y2={HEIGHT - PAD_BOTTOM + 3}
                    className="stroke-muted-foreground/40"
                    strokeWidth={1}
                  />
                  <text
                    x={x}
                    y={HEIGHT - PAD_BOTTOM + 14}
                    textAnchor="middle"
                    className="fill-muted-foreground text-[9px] font-mono"
                  >
                    {tick}
                  </text>
                </g>
              );
            })}

            {/* Area fill under the curve so the drop is visually obvious */}
            <path
              d={`${path} L ${WIDTH - PAD_RIGHT} ${HEIGHT - PAD_BOTTOM} L ${PAD_LEFT} ${HEIGHT - PAD_BOTTOM} Z`}
              className="fill-primary/10"
            />
            <path d={path} className="stroke-primary" strokeWidth={2} fill="none" />

            {/* Crosshair + current point */}
            <line
              x1={cursorX}
              x2={cursorX}
              y1={PAD_TOP}
              y2={HEIGHT - PAD_BOTTOM}
              className="stroke-primary/40"
              strokeDasharray="2 3"
              strokeWidth={1}
            />
            <circle cx={cursorX} cy={cursorY} r={5} className="fill-background stroke-primary" strokeWidth={2} />
          </svg>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <label htmlFor={daySliderId} className="block text-xs font-medium text-foreground mb-1">
              {t('tutorial.curve.daysLabel', { days: day })}
            </label>
            <input
              id={daySliderId}
              type="range"
              min={0}
              max={MAX_DAYS}
              step={1}
              value={day}
              onChange={(e) => onDay(Number(e.target.value))}
              className="w-full"
              aria-label={t('tutorial.curve.daysAria')}
            />
            <div className="text-[10px] text-muted-foreground mt-0.5">{t('tutorial.curve.daysHint')}</div>
          </div>
          <div>
            <label htmlFor={stabSliderId} className="block text-xs font-medium text-foreground mb-1">
              {t('tutorial.curve.stabilityLabel', { stability })}
            </label>
            <input
              id={stabSliderId}
              type="range"
              min={1}
              max={60}
              step={1}
              value={stability}
              onChange={(e) => onStab(Number(e.target.value))}
              className="w-full"
              aria-label={t('tutorial.curve.stabilityAria')}
            />
            <div className="text-[10px] text-muted-foreground mt-0.5">{t('tutorial.curve.stabilityHint')}</div>
          </div>
        </div>

        <div className="px-3 py-2 rounded-lg bg-primary/5 border border-primary/10 text-xs text-foreground leading-relaxed">
          <span className="font-mono text-primary text-sm">{(currentR * 100).toFixed(1)}%</span>{' '}
          <span className="text-muted-foreground">
            {t('tutorial.curve.readout', { retention: (currentR * 100).toFixed(1), days: day, stability })}
          </span>
        </div>
      </CardContent>
    </Card>
  );
}
