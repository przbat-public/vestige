import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { getDateRange } from '@/graph/temporal';
import type { GraphNode } from '@/types';

interface Props {
  nodes: GraphNode[];
  onDateChange: (date: Date) => void;
  onToggle: (enabled: boolean) => void;
}

function formatDate(d: Date, locale: string): string {
  return d.toLocaleDateString(locale, { month: 'short', day: 'numeric', year: 'numeric' });
}

export function TimeSlider({ nodes, onDateChange, onToggle }: Props) {
  const { t, i18n } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [sliderValue, setSliderValue] = useState(100);
  const animFrameRef = useRef(0);
  const lastTimeRef = useRef(0);

  const dateRange = getDateRange(nodes);
  const oldest = dateRange.oldest.getTime();
  const newest = dateRange.newest.getTime();
  const range = newest - oldest || 1;
  const currentDate = new Date(oldest + (sliderValue / 100) * range);
  const locale = i18n.language === 'pl' ? 'pl-PL' : 'en-US';

  const toggle = useCallback(() => {
    const next = !enabled;
    setEnabled(next);
    onToggle(next);
    if (next) {
      setSliderValue(100);
      onDateChange(new Date(newest));
    }
  }, [enabled, onToggle, onDateChange, newest]);

  const playLoop = useCallback(
    (now: number) => {
      const delta = (now - lastTimeRef.current) / 1000;
      lastTimeRef.current = now;
      const totalDays = (newest - oldest) / (24 * 60 * 60 * 1000) || 1;
      const percentPerSecond = (speed / totalDays) * 100;

      setSliderValue((prev) => {
        const next = Math.min(100, prev + percentPerSecond * delta);
        if (next >= 100) {
          setPlaying(false);
          return 100;
        }
        return next;
      });

      animFrameRef.current = requestAnimationFrame(playLoop);
    },
    [speed, oldest, newest],
  );

  useEffect(() => {
    if (playing) {
      lastTimeRef.current = performance.now();
      animFrameRef.current = requestAnimationFrame(playLoop);
    }
    return () => cancelAnimationFrame(animFrameRef.current);
  }, [playing, playLoop]);

  useEffect(() => {
    if (enabled) {
      const date = new Date(oldest + (sliderValue / 100) * range);
      onDateChange(date);
    }
  }, [onDateChange, enabled, sliderValue, oldest, range]);

  const togglePlay = () => {
    if (!playing) {
      setSliderValue(0);
      setPlaying(true);
    } else {
      setPlaying(false);
    }
  };

  if (!enabled) {
    return (
      <button
        type="button"
        onClick={toggle}
        className="absolute bottom-4 right-4 z-10 px-3 py-2 glass rounded-xl text-muted-foreground text-xs hover:text-foreground transition flex items-center gap-1.5"
      >
        <span>◷</span>
        <span>{t('timeSlider.timeline')}</span>
      </button>
    );
  }

  return (
    <div className="absolute bottom-4 left-1/2 -translate-x-1/2 z-10 w-[90%] max-w-xl">
      <div className="glass-panel rounded-xl p-3 space-y-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={togglePlay}
              className="w-7 h-7 rounded-lg bg-primary/20 border border-primary/30 text-primary text-xs flex items-center justify-center hover:bg-primary/30 transition"
              aria-label={playing ? t('timeSlider.pause') : t('timeSlider.play')}
            >
              {playing ? '⏸' : '▶'}
            </button>
            <label className="sr-only" htmlFor="speed-select">
              {t('timeSlider.playbackSpeed')}
            </label>
            <select
              id="speed-select"
              value={speed}
              onChange={(e) => setSpeed(Number(e.target.value))}
              className="px-2 py-1 bg-muted border border-border rounded-lg text-xs text-muted-foreground focus:outline-none"
            >
              <option value={1}>1x</option>
              <option value={7}>7x</option>
              <option value={30}>30x</option>
            </select>
          </div>
          <span className="text-xs text-foreground font-medium">{formatDate(currentDate, locale)}</span>
          <button
            type="button"
            onClick={toggle}
            className="text-xs text-muted-foreground hover:text-foreground transition"
          >
            {t('timeSlider.close')}
          </button>
        </div>
        <input
          type="range"
          min="0"
          max="100"
          step="0.1"
          value={sliderValue}
          onChange={(e) => setSliderValue(Number(e.target.value))}
          aria-label={t('timeSlider.timelinePosition')}
          className="w-full h-1.5 appearance-none bg-muted rounded-full cursor-pointer
            [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3
            [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary
            [&::-webkit-slider-thumb]:shadow-[0_0_8px_rgba(129,140,248,0.4)]"
        />
        <div className="flex justify-between text-xs text-muted-foreground">
          <span>{formatDate(dateRange.oldest, locale)}</span>
          <span>{formatDate(dateRange.newest, locale)}</span>
        </div>
      </div>
    </div>
  );
}
