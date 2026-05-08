import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api } from '@/stores/api';
import { toast } from '@/stores/toast';
import { queryKeys } from '@/stores/query';

/**
 * Build a consistent error toast handler that shows the underlying message
 * when available, instead of swallowing it under a generic translation key.
 *
 * Centralized here (vs duplicating in every mutation) so that toast messages
 * stay symmetric across promote/demote/remove/update — and so future helpers
 * pick up the same shape without each author re-deciding.
 */
function makeErrorHandler(fallback: () => string) {
  return (err: unknown) => {
    const message = err instanceof Error ? err.message : fallback();
    toast(message, 'error');
  };
}

export function useMemoryMutations(options?: {
  onPromote?: () => void;
  onDemote?: () => void;
  onDelete?: () => void;
  onUpdate?: () => void;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ['memories'] });
    qc.invalidateQueries({ queryKey: ['graph'] });
    qc.invalidateQueries({ queryKey: queryKeys.stats });
  };

  const onError = makeErrorHandler(() => t('common.error'));

  const promote = useMutation({
    mutationFn: (id: string) => api.memories.promote(id),
    onSuccess: () => {
      toast(t('memories.promoted'), 'success');
      invalidate();
      options?.onPromote?.();
    },
    onError,
  });

  const demote = useMutation({
    mutationFn: (id: string) => api.memories.demote(id),
    onSuccess: () => {
      toast(t('memories.demotedToast'), 'success');
      invalidate();
      options?.onDemote?.();
    },
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.memories.delete(id),
    onSuccess: () => {
      toast(t('memories.deleted'), 'success');
      invalidate();
      options?.onDelete?.();
    },
    onError,
  });

  const update = useMutation({
    mutationFn: (input: { id: string; content?: string; tags?: string[] }) =>
      api.memories.update(input.id, { content: input.content, tags: input.tags }),
    onSuccess: (memory) => {
      toast(t('memories.updatedToast'), 'success');
      // The single memory query is cached by id; refresh it eagerly so the
      // detail panel reflects new content/tags without a round-trip.
      qc.setQueryData(queryKeys.memory(memory.id), memory);
      invalidate();
      options?.onUpdate?.();
    },
    onError,
  });

  return { promote, demote, remove, update };
}
