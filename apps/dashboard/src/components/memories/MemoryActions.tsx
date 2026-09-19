import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';

interface Props {
  onEdit: () => void;
  onPromote: () => void;
  onDemote: () => void;
  onDelete: () => void;
  onTogglePin: () => void;
  isPinned: boolean;
  promotePending: boolean;
  demotePending: boolean;
  deletePending: boolean;
}

/** Top-right action toolbar shown when the memory is not in edit mode. */
export function MemoryActions({
  onEdit,
  onPromote,
  onDemote,
  onDelete,
  onTogglePin,
  isPinned,
  promotePending,
  demotePending,
  deletePending,
}: Props) {
  const { t } = useTranslation();
  return (
    <div className="flex gap-1">
      <Button
        variant="ghost"
        size="sm"
        onClick={onTogglePin}
        aria-label={isPinned ? t('memories.unpin') : t('memories.pin')}
        aria-pressed={isPinned}
        className={isPinned ? 'text-amber-500 hover:text-amber-600' : 'text-muted-foreground hover:text-foreground'}
        title={isPinned ? t('memories.unpin') : t('memories.pin')}
      >
        {isPinned ? '★' : '☆'}
      </Button>
      <Button variant="secondary" size="sm" onClick={onEdit} aria-label={t('memories.edit')}>
        ✎ {t('memories.edit')}
      </Button>
      <Button variant="success" size="sm" onClick={onPromote} disabled={promotePending}>
        ↑ {t('memories.promote')}
      </Button>
      <Button variant="danger" size="sm" onClick={onDemote} disabled={demotePending}>
        ↓ {t('memories.demote')}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        onClick={onDelete}
        disabled={deletePending}
        className="text-muted-foreground hover:text-destructive"
        aria-label={t('common.delete')}
      >
        ✕
      </Button>
    </div>
  );
}
