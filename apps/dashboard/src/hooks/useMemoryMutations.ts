import { type QueryKey, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { EVENT, track } from '@/stores/telemetry';
import { toast } from '@/stores/toast';

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

/**
 * Window during which a destructive action (delete) can be undone before the
 * backend call actually fires. 5s matches Gmail's industry-standard undo UX
 * — long enough for "wait, no" reflexes, short enough that the user doesn't
 * forget what they undid.
 */
const UNDO_WINDOW_MS = 5000;

export interface MemoryMutationsCallbacks {
  onPromote?: () => void;
  onDemote?: () => void;
  onDelete?: () => void;
  onUpdate?: () => void;
}

export function useMemoryMutations(options?: MemoryMutationsCallbacks) {
  const { t } = useTranslation();
  const qc = useQueryClient();

  // Hoist the options bag into a ref so consumers can pass freshly-created
  // callback objects (which is the React-idiomatic style) without breaking
  // the memoisation of `removeMutate`. Without this, every parent re-render
  // recreates `removeMutate` (because `options` would be in its dep array),
  // which in turn invalidates downstream `useCallback`/`memo` chains.
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const invalidate = useCallback(() => {
    qc.invalidateQueries({ queryKey: ['memories'] });
    qc.invalidateQueries({ queryKey: ['graph'] });
    qc.invalidateQueries({ queryKey: queryKeys.stats });
  }, [qc]);

  const onError = makeErrorHandler(() => t('common.error'));

  const promote = useMutation({
    mutationFn: (id: string) => api.memories.promote(id),
    onSuccess: () => {
      toast(t('memories.promoted'), 'success');
      invalidate();
      optionsRef.current?.onPromote?.();
    },
    onError,
  });

  /**
   * Demote with undo. The API call commits immediately (the memory is just
   * re-ranked, not destroyed), and the toast offers a one-click re-promote so
   * accidental clicks don't punish a memory's retention strength.
   */
  const demote = useMutation({
    mutationFn: (id: string) => api.memories.demote(id),
    onSuccess: (_, id) => {
      toast(t('memories.demotedToast'), 'success', {
        action: {
          label: t('common.undo'),
          onAction: () => {
            track(EVENT.memory_demote_undo);
            promote.mutate(id);
            toast(t('memories.demoteUndone'), 'info', { duration: 2500 });
          },
        },
      });
      invalidate();
      optionsRef.current?.onDemote?.();
    },
    onError,
  });

  /**
   * Deferred delete with undo. Mirrors Gmail's "Message deleted — Undo"
   * pattern: optimistically remove from cache so the UI feels instant, then
   * commit the actual `DELETE /memories/:id` after the undo window. Clicking
   * Undo cancels the timer and restores the cache from a pre-mutation snapshot
   * — no roundtrip to the server, no risk of resurrecting an already-deleted
   * row.
   *
   * Implemented manually (vs `useMutation`) because the standard mutation
   * lifecycle has no notion of "I'm pending, but the network call hasn't
   * happened yet and the caller can change their mind". The returned shape
   * mimics useMutation just enough (`mutate`, `isPending`) for existing
   * call-sites (`MemoryDetail`) to stay untouched.
   */
  const [removePending, setRemovePending] = useState(false);
  // Single-slot bookkeeping. If a user fires another delete before the
  // previous one's undo window expires, we let the previous one commit
  // immediately (no surprise resurrection) and start a fresh window for
  // the new one — matches Gmail's behaviour exactly.
  const pendingDeleteRef = useRef<{
    timer: ReturnType<typeof setTimeout>;
    commit: () => void;
  } | null>(null);
  // Count of in-flight deletes (timer-waiting + API-call-in-flight). We
  // toggle `removePending` based on this count instead of a boolean so that
  // when two deletes overlap, the previous one's terminal cleanup can't
  // accidentally clear the new one's pending flag.
  const inFlightCountRef = useRef(0);

  const bumpInFlight = useCallback((delta: 1 | -1) => {
    inFlightCountRef.current = Math.max(0, inFlightCountRef.current + delta);
    setRemovePending(inFlightCountRef.current > 0);
  }, []);

  // If the hook unmounts (e.g. user navigates away mid-undo-window), flush
  // the pending delete so the user's intent isn't silently dropped.
  useEffect(() => {
    return () => {
      pendingDeleteRef.current?.commit();
    };
  }, []);

  const removeMutate = useCallback(
    (id: string) => {
      // Commit any previously-pending delete first so we don't end up with
      // two in-flight undo windows fighting over the same cache snapshot.
      pendingDeleteRef.current?.commit();
      pendingDeleteRef.current = null;

      // Snapshot every cached list/detail entry that might reference this
      // memory. We restore from these snapshots on undo or on backend error.
      // Targeting `['memories']` covers list views; `queryKeys.memory(id)`
      // covers the detail panel.
      const snapshots: Array<[QueryKey, unknown]> = [];
      for (const q of qc.getQueryCache().findAll({ queryKey: ['memories'] })) {
        snapshots.push([q.queryKey, q.state.data]);
      }

      // Optimistically strip from any cached list. We don't know the exact
      // shape (different endpoints return different envelopes), so handle the
      // two we actually emit: `{ items: Memory[] }` and bare `Memory[]`.
      qc.setQueriesData<unknown>({ queryKey: ['memories'] }, (old: unknown) => {
        if (old && typeof old === 'object' && 'items' in old) {
          const envelope = old as { items: Array<{ id: string }> };
          return {
            ...envelope,
            items: envelope.items.filter((m) => m?.id !== id),
          };
        }
        if (Array.isArray(old)) {
          return (old as Array<{ id: string }>).filter((m) => m?.id !== id);
        }
        return old;
      });

      track(EVENT.memory_delete);
      bumpInFlight(1);
      let committed = false;
      let cancelled = false;

      const restore = () => {
        for (const [key, data] of snapshots) qc.setQueryData(key, data);
      };

      const commit = () => {
        if (committed || cancelled) return;
        committed = true;
        clearTimeout(pendingDeleteRef.current?.timer as ReturnType<typeof setTimeout>);
        pendingDeleteRef.current = null;
        api.memories
          .delete(id)
          .then(() => {
            invalidate();
            optionsRef.current?.onDelete?.();
          })
          .catch((err) => {
            const msg = err instanceof Error ? err.message : t('common.error');
            toast(msg, 'error');
            restore();
            invalidate();
          })
          .finally(() => bumpInFlight(-1));
      };

      const timer = setTimeout(commit, UNDO_WINDOW_MS);
      pendingDeleteRef.current = { timer, commit };

      toast(t('memories.deletedPending'), 'success', {
        duration: UNDO_WINDOW_MS,
        action: {
          label: t('common.undo'),
          onAction: () => {
            if (committed) return;
            cancelled = true;
            track(EVENT.memory_delete_undo);
            clearTimeout(timer);
            pendingDeleteRef.current = null;
            restore();
            bumpInFlight(-1);
            toast(t('memories.deleteUndone'), 'info', { duration: 2500 });
          },
        },
      });
    },
    // `optionsRef` is read through `.current`, so it doesn't belong in deps;
    // the rest are genuinely stable (`qc`, `invalidate`, `bumpInFlight`) or
    // change with the user's locale (`t`).
    [qc, invalidate, t, bumpInFlight],
  );

  const remove = { mutate: removeMutate, isPending: removePending };

  const update = useMutation({
    mutationFn: (input: { id: string; content?: string; tags?: string[] }) =>
      api.memories.update(input.id, { content: input.content, tags: input.tags }),
    onSuccess: (memory) => {
      toast(t('memories.updatedToast'), 'success');
      // The single memory query is cached by id; refresh it eagerly so the
      // detail panel reflects new content/tags without a round-trip.
      qc.setQueryData(queryKeys.memory(memory.id), memory);
      invalidate();
      optionsRef.current?.onUpdate?.();
    },
    onError,
  });

  return { promote, demote, remove, update };
}
