import { useTranslation } from 'react-i18next';
import { ProgressBar } from '@/components/ui/progress-bar';
import type { Memory } from '@/types';
import { NODE_TYPE_COLORS, EPISTEMIC_STATUS_COLORS, retentionColor } from '@/types';

interface MemoryListItemProps {
  memory: Memory;
  isSelected: boolean;
  onSelect: (m: Memory) => void;
}

export function MemoryListItem({ memory, isSelected, onSelect }: MemoryListItemProps) {
  const { t } = useTranslation();

  return (
    <button
      type="button"
      onClick={() => onSelect(memory)}
      className={`w-full text-left px-3 py-2.5 rounded-lg transition min-w-0 ${
        isSelected
          ? 'bg-primary/10 border-l-2 border-primary'
          : 'hover:bg-accent'
      }`}
      aria-expanded={isSelected}
    >
      <div className="flex items-center gap-2">
        <span
          className="w-2 h-2 rounded-full flex-shrink-0"
          style={{ backgroundColor: NODE_TYPE_COLORS[memory.nodeType] || '#8B95A5' }}
          aria-hidden="true"
        />
        <span className="text-xs text-foreground truncate">{memory.content.slice(0, 80)}</span>
      </div>
      <div className="flex items-center gap-2 mt-1 text-xs">
        <span className="text-muted-foreground">
          {t(`nodeTypes.${memory.nodeType}`, { defaultValue: memory.nodeType })}
        </span>
        {memory.epistemicStatus && (
          <span
            className="text-xs"
            style={{ color: EPISTEMIC_STATUS_COLORS[memory.epistemicStatus] || '#8B95A5' }}
          >
            {t(`epistemic.${memory.epistemicStatus}`)}
          </span>
        )}
        <ProgressBar
          value={memory.retentionStrength * 100}
          label={t('memories.retention')}
          color={retentionColor(memory.retentionStrength)}
          className="flex-1 max-w-[80px]"
        />
      </div>
    </button>
  );
}
