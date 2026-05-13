import { useTranslation } from 'react-i18next';
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
      value: memory.retentionStrength,
      color: retentionColor(memory.retentionStrength),
    },
    { label: t('memories.storage'), value: memory.storageStrength },
    { label: t('memories.retrieval'), value: memory.retrievalStrength },
  ];

  return (
    <div className="space-y-2">
      {strengths.map((s) => (
        <div key={s.label}>
          <div className="flex justify-between text-xs text-muted-foreground mb-0.5">
            <span>{s.label}</span>
            <span className="tabular-nums">{(s.value * 100).toFixed(1)}%</span>
          </div>
          <ProgressBar value={s.value * 100} label={s.label} color={s.color} showValue={false} />
        </div>
      ))}
    </div>
  );
}
