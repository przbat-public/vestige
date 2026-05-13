import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { RetentionCurve } from '@/components/RetentionCurve';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useMemoryMutations } from '@/hooks/useMemoryMutations';
import type { Memory } from '@/types';
import { EPISTEMIC_STATUS_COLORS, MEMORY_SYSTEM_COLORS, NODE_TYPE_COLORS } from '@/types';
import { MemoryActions } from './MemoryActions';
import { MemoryChangelogPanel } from './MemoryChangelogPanel';
import { MemoryEditForm } from './MemoryEditForm';
import { MemoryLocalGraph } from './MemoryLocalGraph';
import { MemoryMarkdownView } from './MemoryMarkdownView';
import { MemoryMetadataFooter } from './MemoryMetadataFooter';
import { MemoryStrengthBars } from './MemoryStrengthBars';
import { parseTagsInput } from './memoryDetailUtils';

interface MemoryDetailProps {
  memory: Memory;
  onUpdate: () => void;
  onClose: () => void;
}

export function MemoryDetail({ memory, onUpdate, onClose }: MemoryDetailProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();

  const [editing, setEditing] = useState(false);
  const [draftContent, setDraftContent] = useState(memory.content);
  const [draftTags, setDraftTags] = useState(memory.tags.join(', '));

  // Reset draft state whenever the active memory changes — selecting a
  // different node should not preserve a stale edit buffer from another one.
  useEffect(() => {
    setEditing(false);
    setDraftContent(memory.content);
    setDraftTags(memory.tags.join(', '));
  }, [memory.content, memory.tags]);

  const { promote, demote, remove, update } = useMemoryMutations({
    onPromote: onUpdate,
    onDemote: onUpdate,
    onDelete: () => {
      onClose();
      onUpdate();
    },
    onUpdate,
  });

  const startEdit = () => {
    setDraftContent(memory.content);
    setDraftTags(memory.tags.join(', '));
    setEditing(true);
  };

  const cancelEdit = () => {
    setEditing(false);
    setDraftContent(memory.content);
    setDraftTags(memory.tags.join(', '));
  };

  const saveEdit = () => {
    const trimmed = draftContent.trim();
    if (!trimmed) return; // Required by backend (returns 400 on empty).
    const nextTags = parseTagsInput(draftTags);
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
            onEdit={startEdit}
            onPromote={() => promote.mutate(memory.id)}
            onDemote={() => demote.mutate(memory.id)}
            onDelete={() => remove.mutate(memory.id)}
            promotePending={promote.isPending}
            demotePending={demote.isPending}
            deletePending={remove.isPending}
          />
        )}
      </div>

      {editing ? (
        <MemoryEditForm
          draftContent={draftContent}
          draftTags={draftTags}
          onContentChange={setDraftContent}
          onTagsChange={setDraftTags}
          onCancel={cancelEdit}
          onSubmit={saveEdit}
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

      <MemoryMetadataFooter memory={memory} />

      <div>
        <div className="text-xs text-muted-foreground mb-1 font-medium">{t('memories.retentionForecast')}</div>
        <RetentionCurve retention={memory.retentionStrength} stability={memory.storageStrength} />
      </div>

      <div className="space-y-1.5">
        <div className="text-xs text-muted-foreground font-medium">{t('memories.localGraph.title')}</div>
        <MemoryLocalGraph memoryId={memory.id} />
      </div>

      <MemoryChangelogPanel memoryId={memory.id} />

      <Button variant="dream" className="w-full" size="sm" onClick={() => navigate(`/explore?from=${memory.id}`)}>
        ◬ {t('memories.exploreConnections')}
      </Button>
    </div>
  );
}
