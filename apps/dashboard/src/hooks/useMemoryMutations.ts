import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api } from '@/stores/api';
import { toast } from '@/stores/toast';
import { queryKeys } from '@/stores/query';

export function useMemoryMutations(options?: {
  onPromote?: () => void;
  onDemote?: () => void;
  onDelete?: () => void;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ['memories'] });
    qc.invalidateQueries({ queryKey: ['graph'] });
    qc.invalidateQueries({ queryKey: queryKeys.stats });
  };

  const promote = useMutation({
    mutationFn: (id: string) => api.memories.promote(id),
    onSuccess: () => {
      toast(t('memories.promoted'), 'success');
      invalidate();
      options?.onPromote?.();
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  const demote = useMutation({
    mutationFn: (id: string) => api.memories.demote(id),
    onSuccess: () => {
      toast(t('memories.demotedToast'), 'success');
      invalidate();
      options?.onDemote?.();
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.memories.delete(id),
    onSuccess: () => {
      toast(t('memories.deleted'), 'success');
      invalidate();
      options?.onDelete?.();
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  return { promote, demote, remove };
}
