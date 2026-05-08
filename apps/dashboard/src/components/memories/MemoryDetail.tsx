import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import ReactMarkdown from 'react-markdown';
import { RetentionCurve } from '@/components/RetentionCurve';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ProgressBar } from '@/components/ui/progress-bar';
import { useMemoryMutations } from '@/hooks/useMemoryMutations';
import type { Memory } from '@/types';
import { EPISTEMIC_STATUS_COLORS, MEMORY_SYSTEM_COLORS, NODE_TYPE_COLORS, retentionColor } from '@/types';
import { MemoryChangelogPanel } from './MemoryChangelogPanel';
import { MemoryLocalGraph } from './MemoryLocalGraph';

interface MemoryDetailProps {
  memory: Memory;
  onUpdate: () => void;
  onClose: () => void;
}

/** Parse the comma/space separated tags input into a clean array. */
function parseTagsInput(value: string): string[] {
  return value
    .split(/[,\n]/)
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
}

/**
 * Human-friendly relative time-until string for FSRS-6 next-review timestamps.
 *
 * - Past dates → "Due now" (the FSRS scheduler treats ≤ now as eligible)
 * - <1 hour    → "in 12 minutes"
 * - <1 day     → "in 5 hours"
 * - ≥1 day     → "in 3 days"
 *
 * Returns null when the input is missing/invalid so callers can skip the badge.
 * Uses Intl.RelativeTimeFormat for proper PL/EN inflection ("za 5 godzin").
 */
function formatNextReview(
  iso: string | undefined | null,
  locale: string,
  dueNowLabel: string,
): { label: string; isOverdue: boolean } | null {
  if (!iso) return null;
  const target = Date.parse(iso);
  if (Number.isNaN(target)) return null;
  const deltaMs = target - Date.now();
  if (deltaMs <= 0) return { label: dueNowLabel, isOverdue: true };

  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  const minutes = Math.round(deltaMs / 60_000);
  if (minutes < 60) return { label: rtf.format(minutes, 'minute'), isOverdue: false };
  const hours = Math.round(deltaMs / 3_600_000);
  if (hours < 24) return { label: rtf.format(hours, 'hour'), isOverdue: false };
  const days = Math.round(deltaMs / 86_400_000);
  return { label: rtf.format(days, 'day'), isOverdue: false };
}

export function MemoryDetail({ memory, onUpdate, onClose }: MemoryDetailProps) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const nextReview = formatNextReview(
    memory.nextReviewAt,
    i18n.language,
    t('memories.nextReviewDueNow'),
  );

  const contentInputId = useId();
  const tagsInputId = useId();

  const [editing, setEditing] = useState(false);
  const [draftContent, setDraftContent] = useState(memory.content);
  const [draftTags, setDraftTags] = useState(memory.tags.join(', '));

  // Reset draft state whenever the active memory changes — selecting a
  // different node should not preserve a stale edit buffer from another one.
  useEffect(() => {
    setEditing(false);
    setDraftContent(memory.content);
    setDraftTags(memory.tags.join(', '));
  }, [memory.id, memory.content, memory.tags]);

  const { promote, demote, remove, update } = useMemoryMutations({
    onPromote: onUpdate,
    onDemote: onUpdate,
    onDelete: () => {
      onClose();
      onUpdate();
    },
    onUpdate,
  });

  const strengths = [
    {
      label: t('memories.retention'),
      value: memory.retentionStrength,
      color: retentionColor(memory.retentionStrength),
    },
    { label: t('memories.storage'), value: memory.storageStrength },
    { label: t('memories.retrieval'), value: memory.retrievalStrength },
  ];

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
    const tagsChanged =
      nextTags.length !== memory.tags.length ||
      nextTags.some((tag, i) => tag !== memory.tags[i]);

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
        <div className="flex gap-1">
          {!editing && (
            <>
              <Button
                variant="secondary"
                size="sm"
                onClick={startEdit}
                aria-label={t('memories.edit')}
              >
                ✎ {t('memories.edit')}
              </Button>
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
            </>
          )}
        </div>
      </div>

      {editing ? (
        <form
          className="space-y-2"
          onSubmit={(e) => {
            e.preventDefault();
            saveEdit();
          }}
        >
          <div className="space-y-1">
            <label
              htmlFor={contentInputId}
              className="text-xs font-medium text-muted-foreground"
            >
              {t('memories.editContentLabel')}
            </label>
            <textarea
              id={contentInputId}
              value={draftContent}
              onChange={(e) => setDraftContent(e.target.value)}
              rows={6}
              className="w-full bg-accent border border-border rounded-lg px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring resize-y"
              aria-label={t('memories.editContentLabel')}
              required
              autoFocus
            />
          </div>
          <div className="space-y-1">
            <label
              htmlFor={tagsInputId}
              className="text-xs font-medium text-muted-foreground"
            >
              {t('memories.editTagsLabel')}
            </label>
            <Input
              id={tagsInputId}
              type="text"
              value={draftTags}
              onChange={(e) => setDraftTags(e.target.value)}
              placeholder={t('memories.editTagsPlaceholder')}
              aria-describedby={`${tagsInputId}-help`}
            />
            <p
              id={`${tagsInputId}-help`}
              className="text-[11px] text-muted-foreground"
            >
              {t('memories.editTagsHint')}
            </p>
          </div>
          <div className="flex items-center justify-end gap-2 pt-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={cancelEdit}
              disabled={update.isPending}
            >
              {t('common.cancel')}
            </Button>
            <Button
              type="submit"
              variant="default"
              size="sm"
              disabled={update.isPending || !draftContent.trim()}
            >
              {update.isPending ? t('common.loading') : t('common.save')}
            </Button>
          </div>
        </form>
      ) : (
        // Memories often contain markdown structure ([Updated YYYY-MM-DD],
        // bullet lists, code blocks, links). Rendering them as markdown makes
        // long-running compound memories actually readable. We do NOT enable
        // raw HTML/CSS — this is a security-conscious default (no rehype-raw).
        // The custom components keep heading sizes inline with the panel's
        // type scale so wrapped content does not blow out of the sheet.
        <div className="text-sm text-foreground leading-relaxed break-words memory-md prose-sm">
          <ReactMarkdown
            components={{
              h1: ({ children }) => <h3 className="text-base font-semibold mb-1.5 mt-3 first:mt-0">{children}</h3>,
              h2: ({ children }) => <h4 className="text-sm font-semibold mb-1 mt-3 first:mt-0">{children}</h4>,
              h3: ({ children }) => <h5 className="text-xs font-semibold mb-1 mt-2 first:mt-0">{children}</h5>,
              p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
              ul: ({ children }) => <ul className="list-disc list-inside space-y-0.5 mb-2">{children}</ul>,
              ol: ({ children }) => <ol className="list-decimal list-inside space-y-0.5 mb-2">{children}</ol>,
              li: ({ children }) => <li className="text-sm">{children}</li>,
              code: ({ className, children, ...props }) => {
                const isBlock = (className ?? '').includes('language-');
                if (isBlock) {
                  return (
                    <pre className="bg-accent rounded-md p-2 my-2 overflow-x-auto text-xs">
                      <code className={className} {...props}>
                        {children}
                      </code>
                    </pre>
                  );
                }
                return (
                  <code className="bg-accent rounded px-1 py-0.5 text-xs font-mono">
                    {children}
                  </code>
                );
              },
              a: ({ children, href }) => (
                <a
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-primary underline underline-offset-2 hover:no-underline"
                >
                  {children}
                </a>
              ),
              blockquote: ({ children }) => (
                <blockquote className="border-l-2 border-border pl-2 italic text-muted-foreground my-2">
                  {children}
                </blockquote>
              ),
            }}
          >
            {memory.content}
          </ReactMarkdown>
        </div>
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
            <ProgressBar
              value={s.value * 100}
              label={s.label}
              color={s.color}
              showValue={false}
            />
          </div>
        ))}
      </div>

      <div className="text-xs text-muted-foreground flex items-center gap-3 flex-wrap">
        <span>
          {t('memories.reviews')}:{' '}
          <span className="text-foreground tabular-nums">{memory.reviewCount ?? 0}</span>
        </span>
        {nextReview && (
          <Badge
            variant={nextReview.isOverdue ? 'warning' : 'secondary'}
            aria-label={t('memories.nextReviewAria', { when: nextReview.label })}
          >
            ⟳ {t('memories.nextReviewLabel')} {nextReview.label}
          </Badge>
        )}
      </div>

      <div>
        <div className="text-xs text-muted-foreground mb-1 font-medium">
          {t('memories.retentionForecast')}
        </div>
        <RetentionCurve
          retention={memory.retentionStrength}
          stability={memory.storageStrength}
        />
      </div>

      <div className="text-xs text-muted-foreground space-y-1">
        <div>
          {t('memories.created')}: {new Date(memory.createdAt).toLocaleString()}
        </div>
        <div>
          {t('memories.updated')}: {new Date(memory.updatedAt).toLocaleString()}
        </div>
        {memory.lastAccessedAt && (
          <div>
            {t('memories.accessed')}: {new Date(memory.lastAccessedAt).toLocaleString()}
          </div>
        )}
      </div>

      <div className="space-y-1.5">
        <div className="text-xs text-muted-foreground font-medium">
          {t('memories.localGraph.title')}
        </div>
        <MemoryLocalGraph memoryId={memory.id} />
      </div>

      <MemoryChangelogPanel memoryId={memory.id} />

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
