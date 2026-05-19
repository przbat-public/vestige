import { useTranslation } from 'react-i18next';
import { EVENT, track } from '@/stores/telemetry';
import { type GlossaryKey, termDefKey, termLabelKey } from './glossary';

interface GlossaryTermProps {
  /** Which glossary entry to surface. */
  termKey: GlossaryKey;
  /**
   * Override the inline label. Useful when the surrounding sentence reads
   * better with a different surface form (e.g. "embeddings" instead of
   * "Embedding"). Falls back to the canonical short label.
   */
  label?: string;
}

/**
 * Inline tooltip-wrapped term. Hover or focus reveals the full definition;
 * the dotted underline is the only affordance we need — heavier styling
 * fights the body copy rhythm.
 *
 * Implementation matches `InfoTooltip` (`components/ui/info-tooltip.tsx`)
 * but stays embedded in the run of text via a `<button>` styled like a
 * link. We don't reuse `InfoTooltip` directly because that component
 * forces an icon trigger, which is wrong for an inline definition lookup.
 */
export function GlossaryTerm({ termKey, label }: GlossaryTermProps) {
  const { t } = useTranslation();
  const surface = label ?? t(termLabelKey(termKey));
  const definition = t(termDefKey(termKey));

  return (
    <span className="relative inline-block group">
      <button
        type="button"
        onClick={() => track(EVENT.tutorial_glossary_open, { term: termKey })}
        onMouseEnter={() => track(EVENT.tutorial_glossary_open, { term: termKey })}
        className="inline border-b border-dotted border-current/60 hover:border-current focus:outline-none focus:ring-1 focus:ring-ring rounded-sm cursor-help bg-transparent text-current font-inherit p-0"
        aria-describedby={`glossary-${termKey}`}
      >
        {surface}
      </button>
      <span
        id={`glossary-${termKey}`}
        role="tooltip"
        className="pointer-events-none opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity absolute bottom-full left-1/2 -translate-x-1/2 mb-2 z-50 w-64 max-w-[calc(100vw-2rem)] rounded-lg border border-border bg-card shadow-lg p-3 text-[11px] leading-relaxed text-card-foreground"
      >
        <span className="block font-semibold text-foreground mb-1">{t(termLabelKey(termKey))}</span>
        {definition}
      </span>
    </span>
  );
}
