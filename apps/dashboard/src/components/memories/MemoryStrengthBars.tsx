import { useTranslation } from 'react-i18next';
import { InfoTooltip } from '@/components/ui/info-tooltip';
import { ProgressBar } from '@/components/ui/progress-bar';
import type { Memory } from '@/types';
import { retentionColor } from '@/types';

interface Props {
  memory: Memory;
}

/** Three FSRS strength bars (retention, storage, retrieval) with locale labels. */
export function MemoryStrengthBars({ memory }: Props) {
  const { t } = useTranslation();
  const strengths = [
    {
      label: t('memories.retention'),
      hint: t('memories.hint.retention'),
      value: memory.retentionStrength,
      color: retentionColor(memory.retentionStrength),
    },
    {
      label: t('memories.storage'),
      hint: t('memories.hint.storage'),
      value: memory.storageStrength,
    },
    {
      label: t('memories.retrieval'),
      hint: t('memories.hint.retrieval'),
      value: memory.retrievalStrength,
    },
  ];

  return (
    <div className="space-y-2">
      {strengths.map((s) => (
        <div key={s.label}>
          <div className="flex justify-between text-xs text-muted-foreground mb-0.5">
            <span className="flex items-center gap-1.5">
              {s.label}
              <InfoTooltip content={s.hint} />
            </span>
            <span className="tabular-nums">{(s.value * 100).toFixed(1)}%</span>
          </div>
          <ProgressBar value={s.value * 100} label={s.label} color={s.color} showValue={false} />
        </div>
      ))}
    </div>
  );
}
