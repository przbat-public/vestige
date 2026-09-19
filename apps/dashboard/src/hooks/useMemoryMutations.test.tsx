import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';

// Stub i18n so we can assert against the (stable) key instead of the
// translated text — the actual translations are exercised in i18n
// parity tests, not here.
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

// Mock the API layer with explicit promise-returning fns so we can drive
// resolve/reject from the test body and observe ordering vs. timers.
vi.mock('@/stores/api', () => ({
  api: {
    memories: {
      promote: vi.fn(),
      demote: vi.fn(),
      delete: vi.fn(),
      update: vi.fn(),
    },
  },
}));

// Capture every toast() call so we can assert messages + action handlers.
const toastMock = vi.fn();
vi.mock('@/stores/toast', () => ({
  toast: (...args: unknown[]) => toastMock(...args),
}));

// Telemetry is fire-and-forget; spy on EVENT.* names rather than the
// localStorage backend.
const trackMock = vi.fn();
vi.mock('@/stores/telemetry', async () => {
  const actual = await vi.importActual<typeof import('@/stores/telemetry')>('@/stores/telemetry');
  return {
    ...actual,
    track: (...args: unknown[]) => trackMock(...args),
  };
});

import { api } from '@/stores/api';
import type { Memory, MemoryListResponse, MemoryStatus } from '@/types';
import { useMemoryMutations } from './useMemoryMutations';

type ToastArgs = [
  string,
  ('success' | 'error' | 'info')?,
  { duration?: number; action?: { label: string; onAction: () => void } }?,
];

function wrapper(qc: QueryClient) {
  return function Provider({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  };
}

function freshClient() {
  // Disable retries so .mutate() rejections settle on the first attempt
  // — otherwise TanStack Query's default retry policy stretches a single
  // mutation across multiple microtasks and confuses fake timers.
  // `gcTime: Infinity` keeps cached entries alive for the test even with
  // zero observers, so snapshot/restore assertions inspect real data
  // instead of garbage-collected `undefined`.
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Number.POSITIVE_INFINITY },
      mutations: { retry: false },
    },
  });
}

// Helper: one entry of the shape `api.memories.list` returns. Fields the
// list view omits (`lastAccessedAt`, `insight`, …) stay absent — that is
// exactly what the backend sends for a paginated list.
function memory(id: string, content = `Memory ${id}`): Memory {
  return {
    id,
    content,
    nodeType: 'fact',
    tags: [],
    retentionStrength: 0.8,
    storageStrength: 0.7,
    retrievalStrength: 0.6,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    epistemicStatus: 'world',
    memorySystem: 'semantic',
  };
}

/**
 * A cached list page. The real cache holds `MemoryListResponseDto`
 * (`{ total, memories }`) — never `{ items }` and never a bare array, which
 * is why the old optimistic-delete test could pass while the production path
 * was a no-op.
 */
function listPage(ids: string[], total = ids.length): MemoryListResponse {
  return { total, memories: ids.map((id) => memory(id)) };
}

/** `MemoryStatusDto` as returned by delete / promote / demote. */
function status(id: string, action: MemoryStatus['action'], retentionStrength: number): MemoryStatus {
  return { ok: true, id, retentionStrength, action };
}

beforeEach(() => {
  toastMock.mockReset();
  trackMock.mockReset();
  vi.mocked(api.memories.promote).mockReset();
  vi.mocked(api.memories.demote).mockReset();
  vi.mocked(api.memories.delete).mockReset();
  vi.mocked(api.memories.update).mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useMemoryMutations — promote/demote', () => {
  it('promote: fires API, shows success toast, calls onPromote, invalidates lists', async () => {
    const qc = freshClient();
    qc.setQueryData(['memories', 'list'], listPage(['a']));
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    vi.mocked(api.memories.promote).mockResolvedValue(status('a', 'promoted', 0.9));
    const onPromote = vi.fn();

    const { result } = renderHook(() => useMemoryMutations({ onPromote }), { wrapper: wrapper(qc) });

    await act(async () => {
      await result.current.promote.mutateAsync('a');
    });

    expect(api.memories.promote).toHaveBeenCalledWith('a');
    expect(toastMock).toHaveBeenCalledWith('memories.promoted', 'success');
    expect(onPromote).toHaveBeenCalledOnce();
    // The hook invalidates three buckets (memories, graph, stats).
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['memories'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['graph'] });
  });

  it('promote: surfaces the underlying error message instead of "common.error"', async () => {
    const qc = freshClient();
    vi.mocked(api.memories.promote).mockRejectedValue(new Error('boom: rate limited'));

    const { result } = renderHook(() => useMemoryMutations(), { wrapper: wrapper(qc) });
    await act(async () => {
      await expect(result.current.promote.mutateAsync('a')).rejects.toThrow('boom: rate limited');
    });

    expect(toastMock).toHaveBeenCalledWith('boom: rate limited', 'error');
  });

  it('demote: emits a toast with an Undo action that re-promotes the memory', async () => {
    const qc = freshClient();
    vi.mocked(api.memories.demote).mockResolvedValue(status('a', 'demoted', 0.4));
    vi.mocked(api.memories.promote).mockResolvedValue(status('a', 'promoted', 0.9));

    const { result } = renderHook(() => useMemoryMutations(), { wrapper: wrapper(qc) });
    await act(async () => {
      await result.current.demote.mutateAsync('a');
    });

    expect(toastMock).toHaveBeenCalled();
    const args = toastMock.mock.calls[0] as ToastArgs;
    expect(args[0]).toBe('memories.demotedToast');
    expect(args[1]).toBe('success');
    const action = args[2]?.action;
    expect(action).toBeDefined();
    expect(action?.label).toBe('common.undo');

    // Invoke Undo → triggers promote + tracks the undo event.
    await act(async () => {
      action?.onAction();
      // Give the in-flight promote a microtask to land before assert.
      await Promise.resolve();
    });
    expect(api.memories.promote).toHaveBeenCalledWith('a');
    expect(trackMock).toHaveBeenCalledWith('memory_demote_undo');
  });
});

describe('useMemoryMutations — deferred delete', () => {
  it('optimistically strips from the list and commits after the undo window', async () => {
    vi.useFakeTimers();
    const qc = freshClient();
    qc.setQueryData(['memories', 'list'], listPage(['a', 'b']));
    vi.mocked(api.memories.delete).mockResolvedValue(status('a', 'deleted', 0.8));
    const onDelete = vi.fn();

    const { result } = renderHook(() => useMemoryMutations({ onDelete }), { wrapper: wrapper(qc) });

    act(() => result.current.remove.mutate('a'));

    // List view immediately reflects the delete without a server round-trip.
    // `total` is decremented too, otherwise the pagination footer keeps
    // advertising a row that is no longer rendered.
    expect(qc.getQueryData(['memories', 'list'])).toEqual(listPage(['b']));
    expect(result.current.remove.isPending).toBe(true);
    // The API call hasn't fired yet — that's the whole point of the window.
    expect(api.memories.delete).not.toHaveBeenCalled();

    // Run the undo timer.
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(api.memories.delete).toHaveBeenCalledWith('a');

    // `waitFor` polls with real timers — switching out of fake timer mode
    // before polling avoids the test hanging forever (vitest's `waitFor`
    // would otherwise schedule retries via setTimeout that never fire).
    vi.useRealTimers();
    await waitFor(() => expect(onDelete).toHaveBeenCalledOnce());
    await waitFor(() => expect(result.current.remove.isPending).toBe(false));
  });

  it('undo within the window restores the list and never fires the API call', async () => {
    vi.useFakeTimers();
    const qc = freshClient();
    qc.setQueryData(['memories', 'list'], listPage(['a', 'b']));
    vi.mocked(api.memories.delete).mockResolvedValue(status('a', 'deleted', 0.8));

    const { result } = renderHook(() => useMemoryMutations(), { wrapper: wrapper(qc) });

    act(() => result.current.remove.mutate('a'));
    // Grab the undo action from the optimistic toast.
    const args = toastMock.mock.calls[0] as ToastArgs;
    const undo = args[2]?.action?.onAction;
    expect(undo).toBeDefined();

    // Click Undo before the window closes.
    act(() => undo?.());

    // List restored, no API call, telemetry registered.
    expect(qc.getQueryData(['memories', 'list'])).toEqual(listPage(['a', 'b']));
    expect(api.memories.delete).not.toHaveBeenCalled();
    expect(trackMock).toHaveBeenCalledWith('memory_delete_undo');

    // Advance past the window — confirm the timer is genuinely cancelled.
    await act(async () => {
      vi.advanceTimersByTime(10_000);
      await Promise.resolve();
    });
    expect(api.memories.delete).not.toHaveBeenCalled();
    expect(result.current.remove.isPending).toBe(false);
  });

  it('leaves cache entries that are not list envelopes untouched', async () => {
    vi.useFakeTimers();
    const qc = freshClient();
    // A stale/foreign shape under the same prefix must not be corrupted (and
    // must not be mistaken for a list page).
    const foreign = { items: [memory('a'), memory('b')] };
    qc.setQueryData(['memories', 'legacy'], foreign);
    vi.mocked(api.memories.delete).mockResolvedValue(status('a', 'deleted', 0.8));

    const { result } = renderHook(() => useMemoryMutations(), { wrapper: wrapper(qc) });
    act(() => result.current.remove.mutate('a'));

    expect(qc.getQueryData(['memories', 'legacy'])).toEqual(foreign);
  });

  it('a second delete commits the first immediately so windows never overlap', async () => {
    vi.useFakeTimers();
    const qc = freshClient();
    qc.setQueryData(['memories', 'list'], listPage(['a', 'b']));
    vi.mocked(api.memories.delete).mockResolvedValue(status('a', 'deleted', 0.8));

    const { result } = renderHook(() => useMemoryMutations(), { wrapper: wrapper(qc) });

    act(() => result.current.remove.mutate('a'));
    expect(api.memories.delete).not.toHaveBeenCalled();

    // Fire a second delete while the first is still in its undo window.
    act(() => result.current.remove.mutate('b'));

    // First delete is forced to commit (we mustn't sit on two unresolved
    // windows fighting over snapshots).
    expect(api.memories.delete).toHaveBeenCalledWith('a');
    // Second delete is still inside its own window.
    expect(api.memories.delete).not.toHaveBeenCalledWith('b');

    // Let the second window expire and flush the promise chain.
    await act(async () => {
      vi.advanceTimersByTime(5000);
      await Promise.resolve();
    });
    expect(api.memories.delete).toHaveBeenCalledWith('b');
  });

  it('restores the snapshot and toasts the error when the API call fails', async () => {
    vi.useFakeTimers();
    const qc = freshClient();
    qc.setQueryData(['memories', 'list'], listPage(['a', 'b']));
    vi.mocked(api.memories.delete).mockRejectedValue(new Error('storage offline'));

    const { result } = renderHook(() => useMemoryMutations(), { wrapper: wrapper(qc) });
    act(() => result.current.remove.mutate('a'));

    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(api.memories.delete).toHaveBeenCalledWith('a');

    // Switch to real timers so `waitFor` can poll for the catch handler
    // to land — see the comment in the success-path test for context.
    vi.useRealTimers();
    await waitFor(() => expect(qc.getQueryData(['memories', 'list'])).toEqual(listPage(['a', 'b'])));
    expect(toastMock).toHaveBeenCalledWith('storage offline', 'error');
  });

  it('flushes the pending delete on unmount so the user does not silently lose intent', async () => {
    vi.useFakeTimers();
    const qc = freshClient();
    qc.setQueryData(['memories', 'list'], listPage(['a']));
    vi.mocked(api.memories.delete).mockResolvedValue(status('a', 'deleted', 0.8));

    const { result, unmount } = renderHook(() => useMemoryMutations(), { wrapper: wrapper(qc) });
    act(() => result.current.remove.mutate('a'));
    expect(api.memories.delete).not.toHaveBeenCalled();

    unmount();

    // Unmount cleanup is supposed to call commit() synchronously; the API
    // call should already be in flight when we return from `unmount`.
    expect(api.memories.delete).toHaveBeenCalledWith('a');
  });
});

describe('useMemoryMutations — update', () => {
  it('writes the returned memory into the per-id cache and invalidates lists', async () => {
    const qc = freshClient();
    const updated = { ...memory('a', 'updated content'), tags: ['x', 'y'] };
    // PATCH answers with `MemoryUpdateResultDto { memory, field }` — mocking a
    // bare Memory here is what kept the "cache under ['memory', undefined]"
    // bug green.
    vi.mocked(api.memories.update).mockResolvedValue({ memory: updated, field: 'content+tags' });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const onUpdate = vi.fn();

    const { result } = renderHook(() => useMemoryMutations({ onUpdate }), { wrapper: wrapper(qc) });

    await act(async () => {
      await result.current.update.mutateAsync({ id: 'a', content: 'updated content', tags: ['x', 'y'] });
    });

    expect(qc.getQueryData(['memory', 'a'])).toEqual(updated);
    // Nothing may be written under the `undefined` id — that was the bug.
    expect(qc.getQueryData(['memory', undefined])).toBeUndefined();
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['memories'] });
    expect(toastMock).toHaveBeenCalledWith('memories.updatedToast', 'success');
    expect(onUpdate).toHaveBeenCalledOnce();
  });
});
