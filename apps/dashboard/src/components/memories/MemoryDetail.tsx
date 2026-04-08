import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ProgressBar } from '@/components/ui/progress-bar';
import { RetentionCurve } from '@/components/RetentionCurve';
import { useMemoryMutations } from '@/hooks/useMemoryMutations';
import type { Memory } from '@/types';
import { NODE_TYPE_COLORS, EPISTEMIC_STATUS_COLORS, MEMORY_SYSTEM_COLORS, retentionColor } from '@/types';

interface MemoryDetailProps {
  memory: Memory;
  onUpdate: () => void;
  onClose: () => void;
}

export function MemoryDetail({ memory, onUpdate, onClose }: MemoryDetailProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();

  const { promote, demote, remove } = useMemoryMutations({
    onPromote: onUpdate,
    onDemote: onUpdate,
    onDelete: () => {
      onClose();
      onUpdate();
    },
  });

  const strengths = [
    { label: t('memories.retention'), value: memory.retentionStrength, color: retentionColor(memory.retentionStrength) },
    { label: t('memories.storage'), value: memory.storageStrength },
    { label: t('memories.retrieval'), value: memory.retrievalStrength },
  ];

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <Badge color={NODE_TYPE_COLORS[memory.nodeType]}>
          {t(`nodeTypes.${memory.nodeType}`, { defaultValue: memory.nodeType })}
        </Badge>
        <div className="flex gap-1">
          <Button
            variant="success"
            size="sm"
            onClick={() => promote.mutate(memory.id)}
            disabled={promote.isPending}
          >
            ↑ {t('memories.promote')}
          </Button>
          <Button
            variant="danger"
            size="sm"
            onClick={() => demote.mutate(memory.id)}
            disabled={demote.isPending}
          >
            ↓ {t('memories.demote')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => remove.mutate(memory.id)}
            disabled={remove.isPending}
            className="text-muted-foreground hover:text-destructive"
            aria-label={t('common.delete')}
          >
            ✕
          </Button>
        </div>
      </div>

      <p className="text-sm text-foreground leading-relaxed break-words">{memory.content}</p>

      {memory.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {memory.tags.map((tag) => (
            <Badge key={tag} variant="secondary">{tag}</Badge>
          ))}
        </div>
      )}

      {(memory.epistemicStatus || memory.memorySystem) && (
        <div className="flex gap-2">
          {memory.epistemicStatus && (
            <Badge color={EPISTEMIC_STATUS_COLORS[memory.epistemicStatus]}>
              {t(`epistemic.${memory.epistemicStatus}`)}
            </Badge>
          )}
          {memory.memorySystem && (
            <Badge color={MEMORY_SYSTEM_COLORS[memory.memorySystem]}>
              {t(`memorySystem.${memory.memorySystem}`)}
            </Badge>
          )}
        </div>
      )}

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

      <div className="text-xs text-muted-foreground">
        {t('memories.reviews')}: <span className="text-foreground tabular-nums">{memory.reviewCount ?? 0}</span>
      </div>

      <div>
        <div className="text-xs text-muted-foreground mb-1 font-medium">{t('memories.retentionForecast')}</div>
        <RetentionCurve retention={memory.retentionStrength} stability={memory.storageStrength} />
      </div>

      <div className="text-xs text-muted-foreground space-y-1">
        <div>{t('memories.created')}: {new Date(memory.createdAt).toLocaleString()}</div>
        <div>{t('memories.updated')}: {new Date(memory.updatedAt).toLocaleString()}</div>
        {memory.lastAccessedAt && (
          <div>{t('memories.accessed')}: {new Date(memory.lastAccessedAt).toLocaleString()}</div>
        )}
      </div>

      <Button
        variant="dream"
        className="w-full"
        size="sm"
        onClick={() => navigate(`/explore?from=${memory.id}`)}
      >
        ◬ {t('memories.exploreConnections')}
      </Button>
    </div>
  );
}
