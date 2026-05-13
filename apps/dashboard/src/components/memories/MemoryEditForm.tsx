import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

interface Props {
  draftContent: string;
  draftTags: string;
  onContentChange: (v: string) => void;
  onTagsChange: (v: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
  pending: boolean;
}

/** Inline edit form for memory content and tags (textarea + comma-separated input). */
export function MemoryEditForm({
  draftContent,
  draftTags,
  onContentChange,
  onTagsChange,
  onCancel,
  onSubmit,
  pending,
}: Props) {
  const { t } = useTranslation();
  const contentInputId = useId();
  const tagsInputId = useId();

  return (
    <form
      className="space-y-2"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit();
      }}
    >
      <div className="space-y-1">
        <label htmlFor={contentInputId} className="text-xs font-medium text-muted-foreground">
          {t('memories.editContentLabel')}
        </label>
        <textarea
          id={contentInputId}
          value={draftContent}
          onChange={(e) => onContentChange(e.target.value)}
          rows={6}
          className="w-full bg-accent border border-border rounded-lg px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring resize-y"
          aria-label={t('memories.editContentLabel')}
          required
        />
      </div>
      <div className="space-y-1">
        <label htmlFor={tagsInputId} className="text-xs font-medium text-muted-foreground">
          {t('memories.editTagsLabel')}
        </label>
        <Input
          id={tagsInputId}
          type="text"
          value={draftTags}
          onChange={(e) => onTagsChange(e.target.value)}
          placeholder={t('memories.editTagsPlaceholder')}
          aria-describedby={`${tagsInputId}-help`}
        />
        <p id={`${tagsInputId}-help`} className="text-[11px] text-muted-foreground">
          {t('memories.editTagsHint')}
        </p>
      </div>
      <div className="flex items-center justify-end gap-2 pt-1">
        <Button type="button" variant="ghost" size="sm" onClick={onCancel} disabled={pending}>
          {t('common.cancel')}
        </Button>
        <Button type="submit" variant="default" size="sm" disabled={pending || !draftContent.trim()}>
          {pending ? t('common.loading') : t('common.save')}
        </Button>
      </div>
    </form>
  );
}
