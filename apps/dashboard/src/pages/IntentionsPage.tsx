import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { toast } from '@/stores/toast';

const TRIGGER_TYPES = ['context', 'time', 'event'] as const;
const PRIORITIES = ['low', 'medium', 'high'] as const;

export function IntentionsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [status, setStatus] = useState('active');
  const [showCreate, setShowCreate] = useState(false);
  const [form, setForm] = useState({
    content: '',
    trigger_type: 'context' as (typeof TRIGGER_TYPES)[number],
    trigger_value: '',
    priority: 'medium' as (typeof PRIORITIES)[number],
    deadline: '',
  });

  const { data: intentionsData, isLoading: loading } = useQuery({
    queryKey: queryKeys.intentions(status),
    queryFn: () => api.intentions(status),
  });
  const intentions = intentionsData?.intentions ?? [];

  const { data: predictions } = useQuery({
    queryKey: queryKeys.predictions,
    queryFn: api.predict,
  });

  const createMutation = useMutation({
    mutationFn: api.createIntention,
    onSuccess: () => {
      toast(t('intentions.created'), 'success');
      setForm({ content: '', trigger_type: 'context', trigger_value: '', priority: 'medium', deadline: '' });
      setShowCreate(false);
      qc.invalidateQueries({ queryKey: queryKeys.intentions(status) });
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.content.trim() || !form.trigger_value.trim()) return;
    createMutation.mutate({
      content: form.content,
      trigger_type: form.trigger_type,
      trigger_value: form.trigger_value,
      priority: form.priority,
      deadline: form.deadline || undefined,
    });
  };

  const priorityVariant = (p: string) => {
    if (p === 'high') return 'danger' as const;
    if (p === 'medium') return 'warning' as const;
    return 'success' as const;
  };

  return (
    <div className="p-4 space-y-4 overflow-y-auto h-full w-full">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-bold text-foreground">{t('intentions.title')}</h2>
        <div className="flex gap-2 items-center">
          {(['active', 'fulfilled', 'all'] as const).map((s) => (
            <button
              type="button"
              key={s}
              onClick={() => setStatus(s)}
              className={`px-3 py-1 rounded-lg text-xs transition ${
                status === s ? 'bg-primary/15 text-primary font-medium' : 'text-muted-foreground hover:text-foreground hover:bg-accent'
              }`}
              aria-pressed={status === s}
            >
              {t(`intentions.status.${s}`, { defaultValue: s })}
            </button>
          ))}
          <Button variant="dream" size="sm" onClick={() => setShowCreate(!showCreate)}>
            {showCreate ? t('common.cancel') : t('common.new')}
          </Button>
        </div>
      </div>

      {showCreate && (
        <Card>
          <form onSubmit={handleCreate} className="space-y-3">
            <div>
              <label htmlFor="intention-content" className="text-xs text-muted-foreground block mb-1">
                {t('intentions.title')}
              </label>
              <input
                id="intention-content"
                type="text"
                value={form.content}
                onChange={(e) => setForm((f) => ({ ...f, content: e.target.value }))}
                placeholder={t('intentions.searchPlaceholder')}
                className="w-full px-3 py-2 rounded-lg text-sm bg-background border border-border text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                required
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label htmlFor="trigger-type" className="text-xs text-muted-foreground block mb-1">
                  {t('intentions.triggerType.context')}
                </label>
                <select
                  id="trigger-type"
                  value={form.trigger_type}
                  onChange={(e) => setForm((f) => ({ ...f, trigger_type: e.target.value as (typeof TRIGGER_TYPES)[number] }))}
                  className="w-full px-3 py-2 rounded-lg text-sm bg-background border border-border text-foreground"
                >
                  {TRIGGER_TYPES.map((type) => (
                    <option key={type} value={type}>{t(`intentions.triggerType.${type}`, { defaultValue: type })}</option>
                  ))}
                </select>
              </div>
              <div>
                <label htmlFor="trigger-value" className="text-xs text-muted-foreground block mb-1">
                  {t('intentions.triggerValue')}
                </label>
                <input
                  id="trigger-value"
                  type="text"
                  value={form.trigger_value}
                  onChange={(e) => setForm((f) => ({ ...f, trigger_value: e.target.value }))}
                  placeholder={form.trigger_type === 'time' ? t('intentions.timePlaceholder') : t('intentions.topicPlaceholder')}
                  className="w-full px-3 py-2 rounded-lg text-sm bg-background border border-border text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  required
                />
              </div>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label htmlFor="priority" className="text-xs text-muted-foreground block mb-1">{t('intentions.priorityLabel')}</label>
                <select
                  id="priority"
                  value={form.priority}
                  onChange={(e) => setForm((f) => ({ ...f, priority: e.target.value as (typeof PRIORITIES)[number] }))}
                  className="w-full px-3 py-2 rounded-lg text-sm bg-background border border-border text-foreground"
                >
                  {PRIORITIES.map((p) => (
                    <option key={p} value={p}>{t(`intentions.priority.${p}`)}</option>
                  ))}
                </select>
              </div>
              <div>
                <label htmlFor="deadline" className="text-xs text-muted-foreground block mb-1">{t('intentions.deadline')}</label>
                <input
                  id="deadline"
                  type="date"
                  value={form.deadline}
                  onChange={(e) => setForm((f) => ({ ...f, deadline: e.target.value }))}
                  className="w-full px-3 py-2 rounded-lg text-sm bg-background border border-border text-foreground"
                />
              </div>
            </div>
            <Button type="submit" variant="default" className="w-full" disabled={createMutation.isPending}>
              {createMutation.isPending ? t('common.loading') : t('common.save')}
            </Button>
          </form>
        </Card>
      )}

      {loading ? (
        <LoadingSpinner label={t('common.loading')} />
      ) : intentions.length === 0 ? (
        <EmptyState icon="◌" title={t('intentions.noIntentions')} description={t('intentions.noIntentionsHint')} />
      ) : (
        <div className="space-y-2">
          {intentions.map((item) => (
            <Card key={item.id} className="space-y-2">
              <div className="flex items-start justify-between gap-2">
                <p className="text-sm text-foreground break-words">{item.content}</p>
                <Badge variant={priorityVariant(item.priority)}>
                  {t(`intentions.priority.${item.priority}`, { defaultValue: item.priority })}
                </Badge>
              </div>
              <div className="flex items-center gap-3 text-xs text-muted-foreground flex-wrap">
                <Badge variant="secondary">
                  {t(`intentions.triggerType.${item.trigger_type}`, { defaultValue: item.trigger_type })}: {item.trigger_value}
                </Badge>
                <Badge variant="outline">{t(`intentions.status.${item.status}`, { defaultValue: item.status })}</Badge>
                {item.deadline && <span>{t('intentions.deadline')}: {new Date(item.deadline).toLocaleDateString()}</span>}
              </div>
            </Card>
          ))}
        </div>
      )}

      {predictions && (
        <Card>
          <h3 className="text-xs font-bold text-foreground mb-2">{t('intentions.predictions')}</h3>
          <pre className="text-xs text-muted-foreground overflow-x-auto max-h-60">
            {JSON.stringify(predictions, null, 2)}
          </pre>
        </Card>
      )}
    </div>
  );
}
