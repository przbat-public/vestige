import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate, useSearchParams } from 'react-router';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SegmentedControl } from '@/components/ui/segmented-control';
import { api } from '@/stores/api';
import { useDialogStore } from '@/stores/dialogs';
import { queryKeys } from '@/stores/query';
import { EVENT, track, useTrackPageView } from '@/stores/telemetry';
import { toast } from '@/stores/toast';
import type { TemporalAction, TemporalEntry } from '@/types';

/**
 * Temporal-versioning console.
 *
 * Surfaces `tools/temporal.rs` — the entire temporal subsystem was
 * exposed only via MCP before this. Three browse modes (current /
 * expired / history) plus the `invalidate` mutation, which marks a
 * memory as superseded so the engine stops returning it as current.
 *
 * History mode is the killer use case: pick a topic, see how the
 * engine's stored facts about that topic evolved over time. This is
 * the closest the dashboard gets to "show me what you used to think".
 */

const TABS: TemporalAction[] = ['current', 'expired', 'history'];

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: temporal page integrates URL deep-linking, three independent tab states, topic submission, optimistic invalidate mutation, and a TimelineDot decorator — extracting any one would leak prop interface complexity to children and obscure the data-flow inside this file
export function TemporalPage() {
  useTrackPageView('temporal');
  const { t } = useTranslation();
  const qc = useQueryClient();
  // Read deep-link params on first render so MemoryTemporalPanel's
  // "Show temporal history for tag X" shortcut lands on the right tab
  // with the topic pre-submitted. We avoid mirroring topic edits back
  // into the URL to keep typing snappy and the history clean.
  const [searchParams] = useSearchParams();
  const initialAction = (searchParams.get('action') as TemporalAction) || 'current';
  const initialTopic = searchParams.get('topic') ?? '';
  const [tab, setTab] = useState<TemporalAction>(TABS.includes(initialAction) ? initialAction : 'current');
  const [topic, setTopic] = useState(initialTopic);
  const [submittedTopic, setSubmittedTopic] = useState(initialTopic);

  // Sync on subsequent navigations (e.g. user jumps from a different
  // memory's panel without leaving the SPA). The dependency array
  // intentionally excludes `tab` / `submittedTopic` so manual changes
  // inside the page aren't fought by stale URL state.
  useEffect(() => {
    const nextAction = (searchParams.get('action') as TemporalAction) || null;
    const nextTopic = searchParams.get('topic');
    if (nextAction && TABS.includes(nextAction)) setTab(nextAction);
    if (nextTopic !== null) {
      setTopic(nextTopic);
      setSubmittedTopic(nextTopic);
    }
  }, [searchParams]);

  const tabOptions = useMemo(
    () =>
      TABS.map((a) => ({
        value: a,
        label: t(`temporal.tab.${a}`),
      })),
    [t],
  );

  // History mode requires a topic — block the query until the user has
  // submitted one. For current/expired, an empty topic is "scan the whole
  // base", which is fine.
  const enabled = tab !== 'history' || submittedTopic.length > 0;

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: queryKeys.temporal(tab, submittedTopic || undefined),
    queryFn: () => api.temporal(tab, { topic: submittedTopic || undefined, limit: 50 }),
    enabled,
  });

  const invalidateMutation = useMutation({
    mutationFn: (memoryId: string) => api.temporal('invalidate', { memoryId }),
    onSuccess: () => {
      toast(t('temporal.invalidated'), 'success');
      track(EVENT.temporal_invalidate);
      // The mutation marks the memory as expired — both the current and
      // expired lists need to refresh, plus the global memory list.
      qc.invalidateQueries({ queryKey: ['temporal'] });
      qc.invalidateQueries({ queryKey: queryKeys.memories() });
    },
    onError: (err: Error) => toast(err.message || t('common.error'), 'error'),
  });

  const onSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setSubmittedTopic(topic.trim());
  };

  const memories = data?.memories ?? [];

  // Same drawer-open contract as HubCard / InsightCard / CommandPalette:
  // stash the requested memory id and route to /memories so the
  // MemoriesPage useEffect drains the slot and opens the detail
  // drawer. Previously the temporal entries were inert — the inline
  // TODO on this page admitted the gap and asked users to copy short
  // ids by hand. That's no longer needed.
  const navigate = useNavigate();
  const openMemory = useCallback(
    (id: string) => {
      useDialogStore.getState().requestSelectMemory(id);
      navigate('/memories');
    },
    [navigate],
  );

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div>
          <h2 className="text-lg font-bold text-foreground">{t('temporal.title')}</h2>
          <p className="text-xs text-muted-foreground mt-0.5">{t('temporal.subtitle')}</p>
        </div>
        <SegmentedControl value={tab} onChange={(v) => setTab(v as TemporalAction)} options={tabOptions} />
      </div>

      <Card>
        <form onSubmit={onSearch} className="flex gap-2 items-end">
          <div className="flex-1">
            <label htmlFor="temporal-topic" className="text-xs text-muted-foreground block mb-1">
              {tab === 'history' ? t('temporal.topicRequired') : t('temporal.topicOptional')}
            </label>
            <Input
              id="temporal-topic"
              value={topic}
              onChange={(e) => setTopic(e.target.value)}
              placeholder={t('temporal.topicPlaceholder')}
            />
          </div>
          <Button type="submit" variant="default">
            {t('temporal.search')}
          </Button>
        </form>
        <p className="text-[10px] text-muted-foreground mt-2">{t(`temporal.help.${tab}`)}</p>
      </Card>

      {isError ? (
        <QueryErrorPanel error={error} onRetry={refetch} />
      ) : !enabled ? (
        <EmptyState
          icon="◴"
          title={t('temporal.historyNeedsTopic')}
          description={t('temporal.historyNeedsTopicHint')}
        />
      ) : isLoading ? (
        <LoadingSpinner label={t('common.loading')} />
      ) : memories.length === 0 ? (
        <EmptyState icon="◌" title={t(`temporal.empty.${tab}`)} description={t(`temporal.emptyHint.${tab}`)} />
      ) : (
        <div className="space-y-2">
          <p className="text-xs text-muted-foreground tabular-nums">
            {t('temporal.resultCount', { count: data?.count ?? memories.length })}
          </p>
          {memories.map((m) => (
            <TemporalEntryCard
              key={`${m.id}-${m.validFrom ?? ''}`}
              entry={m}
              tab={tab}
              onInvalidate={() => invalidateMutation.mutate(m.id)}
              invalidating={invalidateMutation.isPending && invalidateMutation.variables === m.id}
              onOpen={() => openMemory(m.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface TemporalEntryCardProps {
  entry: TemporalEntry;
  tab: TemporalAction;
  onInvalidate: () => void;
  invalidating: boolean;
  onOpen: () => void;
}

function TemporalEntryCard({ entry, tab, onInvalidate, invalidating, onOpen }: TemporalEntryCardProps) {
  const { t } = useTranslation();

  // Badge logic mirrors `MemoryMetadataFooter` — expired vs expiring vs
  // valid. We compute days here rather than relying on `daysExpired` so
  // the badge stays consistent across the three actions (history doesn't
  // populate `daysExpired`).
  const badge = useMemo(() => {
    if (!entry.validUntil) return null;
    const now = Date.now();
    const until = new Date(entry.validUntil).getTime();
    const days = Math.round((until - now) / (24 * 60 * 60 * 1000));
    if (days < 0) {
      return { variant: 'danger' as const, label: t('memories.temporalExpired', { days: Math.abs(days) }) };
    }
    if (days < 30) {
      return { variant: 'warning' as const, label: t('memories.temporalExpiring', { days }) };
    }
    return null;
  }, [entry.validUntil, t]);

  return (
    <Card className="space-y-2">
      <div className="flex items-start justify-between gap-3">
        {/* Click-through into the standard MemoriesPage detail drawer.
            Pre-v3.4.2 this was inert — the page rendered the snippet as
            a static `<p>` and the inline TODO asked users to copy short
            ids out of the footer by hand. Now we stash the id in the
            dialog store and route through, same pattern as HubCard /
            InsightCard / CommandPalette. */}
        <button
          type="button"
          onClick={onOpen}
          className="flex-1 text-sm text-foreground break-words text-left cursor-pointer hover:underline"
        >
          {entry.content}
        </button>
        <div className="flex flex-col items-end gap-1 shrink-0">
          {badge && <Badge variant={badge.variant}>{badge.label}</Badge>}
          <span className="text-[10px] text-muted-foreground/80 tabular-nums">r{entry.retention.toFixed(2)}</span>
        </div>
      </div>
      {entry.tags.length > 0 && (
        <div className="flex gap-1 flex-wrap">
          {entry.tags.slice(0, 6).map((tag) => (
            <Badge key={tag} variant="secondary" className="text-[10px]">
              {tag}
            </Badge>
          ))}
        </div>
      )}
      <div className="flex items-center justify-between text-[10px] text-muted-foreground">
        <span className="tabular-nums">
          {entry.validFrom && t('temporal.validFrom', { date: new Date(entry.validFrom).toLocaleString() })}
          {entry.validUntil && (
            <>
              {entry.validFrom && ' · '}
              {t('temporal.validUntil', { date: new Date(entry.validUntil).toLocaleString() })}
            </>
          )}
        </span>
        {tab !== 'expired' && (
          <button
            type="button"
            onClick={onInvalidate}
            disabled={invalidating}
            className="text-[10px] text-amber-600 dark:text-amber-400 hover:underline disabled:opacity-50"
          >
            {invalidating ? t('common.loading') : t('temporal.invalidate')}
          </button>
        )}
      </div>
    </Card>
  );
}
