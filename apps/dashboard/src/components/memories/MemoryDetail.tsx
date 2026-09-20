import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { RetentionCurve } from '@/components/RetentionCurve';
import { Alert } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useMemoryMutations } from '@/hooks/useMemoryMutations';
import { togglePinned, usePinned } from '@/stores/pinned';
import { queryKeys } from '@/stores/query';
import { EVENT, track } from '@/stores/telemetry';
import type { Memory } from '@/types';
import { EPISTEMIC_STATUS_COLORS, MEMORY_SYSTEM_COLORS, NODE_TYPE_COLORS } from '@/types';
import { MemoryActions } from './MemoryActions';
import { MemoryChangelogPanel } from './MemoryChangelogPanel';
import { MemoryEditPanel } from './MemoryEditPanel';
import { MemoryLocalGraph } from './MemoryLocalGraph';
import { MemoryMarkdownView } from './MemoryMarkdownView';
import { MemoryMetadataFooter } from './MemoryMetadataFooter';
import { MemoryRevisionsPanel } from './MemoryRevisionsPanel';
import { MemoryStrengthBars } from './MemoryStrengthBars';
import { MemoryTemporalPanel } from './MemoryTemporalPanel';
import { parseTagsInput } from './memoryDetailUtils';
import { SelfContainedFindings } from './SelfContainedFindings';

interface MemoryDetailProps {
  memory: Memory;
  onUpdate: () => void;
  onClose: () => void;
}

export function MemoryDetail({ memory: listSnapshot, onUpdate, onClose }: MemoryDetailProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const pinned = usePinned();

  /**
   * The caller owns the "which memory is open" selection and passes the row
   * object it already has. That object is a *snapshot*: after a successful
   * save, the list row (and therefore this prop) still holds the pre-edit
   * content, so rendering the prop directly made a successful save look like
   * a no-op.
   *
   * `useMemoryMutations.update` writes the authoritative post-update memory
   * into `queryKeys.memory(id)`. Read through that cache entry with the prop
   * as `initialData`, and the panel re-renders with the saved value without
   * asking the caller to keep its selection state in sync. `initialData` (not
   * a fetch) keeps the existing behaviour: no network request here, and no
   * `staleTime: 0` refetch storm.
   */
  const { data: memory } = useQuery({
    queryKey: queryKeys.memory(listSnapshot.id),
    queryFn: () => listSnapshot,
    initialData: listSnapshot,
    // Never treat the seeded snapshot as fresh enough to re-fetch from here;
    // the mutations/WS layer owns invalidation.
    staleTime: Number.POSITIVE_INFINITY,
  });

  const isPinned = pinned.has(memory.id);

  const handleTogglePin = () => {
    const nowPinned = togglePinned(memory.id);
    track(nowPinned ? EVENT.memory_pin : EVENT.memory_unpin);
  };

  const [editing, setEditing] = useState(false);

  // Leaving edit mode when a different memory is selected. The draft buffer
  // itself lives in `MemoryEditPanel`, keyed on the id, so a stale buffer from
  // another row is impossible — and a save landing through the cache can't
  // clobber what the user is currently typing.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the effect *is* keyed on the id — this component is reused (not remounted) when the caller selects another row, so edit mode must be reset explicitly
  useEffect(() => {
    setEditing(false);
  }, [memory.id]);

  const { promote, demote, remove, update } = useMemoryMutations({
    onPromote: onUpdate,
    onDemote: onUpdate,
    onDelete: () => {
      onClose();
      onUpdate();
    },
    onUpdate,
  });

  const saveEdit = (draft: { content: string; tags: string }) => {
    const trimmed = draft.content.trim();
    if (!trimmed) return; // Required by backend (returns 400 on empty).
    const nextTags = parseTagsInput(draft.tags);
    const contentChanged = trimmed !== memory.content;
    const tagsChanged = nextTags.length !== memory.tags.length || nextTags.some((tag, i) => tag !== memory.tags[i]);

    if (!contentChanged && !tagsChanged) {
      setEditing(false);
      return;
    }

    update.mutate(
      {
        id: memory.id,
        ...(contentChanged ? { content: trimmed } : {}),
        ...(tagsChanged ? { tags: nextTags } : {}),
      },
      {
        onSuccess: () => setEditing(false),
      },
    );
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <Badge color={NODE_TYPE_COLORS[memory.nodeType]}>
          {t(`nodeTypes.${memory.nodeType}`, { defaultValue: memory.nodeType })}
        </Badge>
        {!editing && (
          <MemoryActions
            onEdit={() => setEditing(true)}
            onPromote={() => promote.mutate(memory.id)}
            onDemote={() => demote.mutate(memory.id)}
            onDelete={() => remove.mutate(memory.id)}
            onTogglePin={handleTogglePin}
            isPinned={isPinned}
            promotePending={promote.isPending}
            demotePending={demote.isPending}
            deletePending={remove.isPending}
          />
        )}
      </div>

      {editing ? (
        <MemoryEditPanel
          key={memory.id}
          memory={memory}
          onSubmit={saveEdit}
          onCancel={() => setEditing(false)}
          pending={update.isPending}
        />
      ) : (
        <MemoryMarkdownView content={memory.content} />
      )}

      {!editing && memory.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {memory.tags.map((tag) => (
            <Badge key={tag} variant="secondary">
              {tag}
            </Badge>
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
            <Badge color={MEMORY_SYSTEM_COLORS[memory.memorySystem]}>{t(`memorySystem.${memory.memorySystem}`)}</Badge>
          )}
        </div>
      )}

      <MemoryStrengthBars memory={memory} />

      {/* The gate's verdict. `selfContained === false` means it ran and flagged
          this memory; `undefined` means it never ran, which is deliberately not
          rendered as "clean". A flagged memory is findable, and a reader who
          cannot see the flag cannot know the entry needs its context. */}
      {memory.selfContained === false && (
        <Alert variant="warning">
          <p className="font-medium mb-1">{t('selfContained.detailTitle')}</p>
          <p className="text-xs">{t('selfContained.detailExplanation')}</p>
          {(memory.selfContainedFindings?.length ?? 0) > 0 && (
            <p className="mt-2 text-xs font-medium">{t('selfContained.findingsTitle')}</p>
          )}
          <SelfContainedFindings findings={memory.selfContainedFindings ?? []} />
        </Alert>
      )}

      <MemoryTemporalPanel memory={memory} />

      <MemoryMetadataFooter memory={memory} />

      <div>
        <div className="text-xs text-muted-foreground mb-1 font-medium">{t('memories.retentionForecast')}</div>
        <RetentionCurve retention={memory.retentionStrength} stability={memory.storageStrength} />
      </div>

      <div className="space-y-1.5">
        <div className="text-xs text-muted-foreground font-medium">{t('memories.localGraph.title')}</div>
        <MemoryLocalGraph memoryId={memory.id} />
      </div>

      {/* Two histories, two panels, each labelled for what it shows: state
          transitions (life cycle) and content revisions (what it used to say). */}
      <MemoryChangelogPanel memoryId={memory.id} />

      <MemoryRevisionsPanel memoryId={memory.id} />

      <Button variant="dream" className="w-full" size="sm" onClick={() => navigate(`/explore?from=${memory.id}`)}>
        ◬ {t('memories.exploreConnections')}
      </Button>
    </div>
  );
}
