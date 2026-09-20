import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';

/**
 * One rule the self-containedness gate fired on a memory.
 *
 * Structurally identical to `SmartIngestFinding` (write response) and
 * `SelfContainedFindingDto` (persisted verdict), which is deliberate: the same
 * list is rendered in the add dialog and in the memory detail, so the reader
 * sees the same `kind` / `span` / `hint` triple in both places.
 */
export interface SelfContainedFinding {
  kind: string;
  span: string;
  hint: string;
}

interface Props {
  findings: readonly SelfContainedFinding[];
}

/**
 * Render the findings behind a flagged memory.
 *
 * A finding is only actionable if the reader can see *which text* tripped the
 * rule (`span`) next to *what to write instead* (`hint`) — that is why the gate
 * returns a triple rather than a sentence, and why this component renders all
 * three instead of the `kind` alone.
 */
export function SelfContainedFindings({ findings }: Props) {
  const { t } = useTranslation();
  if (findings.length === 0) return null;

  return (
    <ul className="mt-2 space-y-1.5">
      {findings.map((finding, index) => (
        <li
          // Two findings can legitimately repeat a kind and span (different
          // rules, same phrase), so the index is part of the key.
          // biome-ignore lint/suspicious/noArrayIndexKey: findings are a static response payload and never reorder
          key={`${finding.kind}|${finding.span}|${index}`}
          className="text-xs leading-relaxed"
        >
          <Badge variant="warning">{finding.kind}</Badge>
          <span className="ml-1.5 break-words opacity-80">
            {t('selfContained.spanLabel')} “{finding.span}”
          </span>
          <span className="block mt-0.5 opacity-90">{finding.hint}</span>
        </li>
      ))}
    </ul>
  );
}
