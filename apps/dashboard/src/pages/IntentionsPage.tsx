import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { useForm } from 'react-hook-form';
import { useTranslation } from 'react-i18next';
import { z } from 'zod';
import { SectionErrorBoundary } from '@/components/SectionErrorBoundary';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import { LoadingSpinner } from '@/components/ui/loading-spinner';
import { NativeSelect } from '@/components/ui/native-select';
import { QueryErrorPanel } from '@/components/ui/query-error-panel';
import { SegmentedControl } from '@/components/ui/segmented-control';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import { toast } from '@/stores/toast';

const TRIGGER_TYPES = ['context', 'time', 'event'] as const;
const PRIORITIES = ['low', 'medium', 'high'] as const;

const intentionSchema = z.object({
  content: z.string().min(1, 'Required'),
  trigger_type: z.enum(TRIGGER_TYPES),
  trigger_value: z.string().min(1, 'Required'),
  priority: z.enum(PRIORITIES),
  deadline: z.string().optional(),
});

type IntentionForm = z.infer<typeof intentionSchema>;

export function IntentionsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [status, setStatus] = useState('active');
  const [showCreate, setShowCreate] = useState(false);

  const {
    register,
    handleSubmit,
    reset,
    watch,
    formState: { errors },
  } = useForm<IntentionForm>({
    resolver: zodResolver(intentionSchema),
    defaultValues: { content: '', trigger_type: 'context', trigger_value: '', priority: 'medium', deadline: '' },
  });

  const triggerType = watch('trigger_type');

  const statusOptions = useMemo(
    () =>
      (['active', 'fulfilled', 'all'] as const).map((s) => ({
        value: s,
        label: t(`intentions.status.${s}`, { defaultValue: s }),
      })),
    [t],
  );

  const {
    data: intentionsData,
    isLoading: loading,
    isError,
    error: intentionsError,
    refetch: refetchIntentions,
  } = useQuery({
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
      reset();
      setShowCreate(false);
      qc.invalidateQueries({ queryKey: queryKeys.intentions(status) });
    },
    onError: () => toast(t('common.error'), 'error'),
  });

  const onSubmit = (data: IntentionForm) => {
    createMutation.mutate({
      ...data,
      deadline: data.deadline || undefined,
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
          <SegmentedControl value={status} onChange={setStatus} options={statusOptions} />
          <Button variant="dream" size="sm" onClick={() => setShowCreate(!showCreate)}>
            {showCreate ? t('common.cancel') : t('common.new')}
          </Button>
        </div>
      </div>

      {showCreate && (
        <Card>
          <form onSubmit={handleSubmit(onSubmit)} className="space-y-3">
            <div>
              <label htmlFor="intention-content" className="text-xs text-muted-foreground block mb-1">
                {t('intentions.title')}
              </label>
              <Input
                id="intention-content"
                {...register('content')}
                placeholder={t('intentions.searchPlaceholder')}
                aria-invalid={!!errors.content}
              />
              {errors.content && <p className="text-xs text-red-500 mt-1">{errors.content.message}</p>}
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label htmlFor="trigger-type" className="text-xs text-muted-foreground block mb-1">
                  {t('intentions.triggerType.context')}
                </label>
                <NativeSelect id="trigger-type" {...register('trigger_type')} className="w-full">
                  {TRIGGER_TYPES.map((type) => (
                    <option key={type} value={type}>
                      {t(`intentions.triggerType.${type}`, { defaultValue: type })}
                    </option>
                  ))}
                </NativeSelect>
              </div>
              <div>
                <label htmlFor="trigger-value" className="text-xs text-muted-foreground block mb-1">
                  {t('intentions.triggerValue')}
                </label>
                <Input
                  id="trigger-value"
                  {...register('trigger_value')}
                  placeholder={
                    triggerType === 'time' ? t('intentions.timePlaceholder') : t('intentions.topicPlaceholder')
                  }
                  aria-invalid={!!errors.trigger_value}
                />
                {errors.trigger_value && <p className="text-xs text-red-500 mt-1">{errors.trigger_value.message}</p>}
              </div>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label htmlFor="priority" className="text-xs text-muted-foreground block mb-1">
                  {t('intentions.priorityLabel')}
                </label>
                <NativeSelect id="priority" {...register('priority')} className="w-full">
                  {PRIORITIES.map((p) => (
                    <option key={p} value={p}>
                      {t(`intentions.priority.${p}`)}
                    </option>
                  ))}
                </NativeSelect>
              </div>
              <div>
                <label htmlFor="deadline" className="text-xs text-muted-foreground block mb-1">
                  {t('intentions.deadline')}
                </label>
                <Input id="deadline" type="date" {...register('deadline')} />
              </div>
            </div>
            <Button type="submit" variant="default" className="w-full" disabled={createMutation.isPending}>
              {createMutation.isPending ? t('common.loading') : t('common.save')}
            </Button>
          </form>
        </Card>
      )}

      {isError ? (
        <QueryErrorPanel error={intentionsError} onRetry={refetchIntentions} />
      ) : loading ? (
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
                  {t(`intentions.triggerType.${item.triggerType}`, { defaultValue: item.triggerType })}:{' '}
                  {item.triggerValue}
                </Badge>
                <Badge variant="outline">{t(`intentions.status.${item.status}`, { defaultValue: item.status })}</Badge>
                {item.deadline && (
                  <span>
                    {t('intentions.deadline')}: {new Date(item.deadline).toLocaleDateString()}
                  </span>
                )}
              </div>
            </Card>
          ))}
        </div>
      )}

      {predictions && (
        <SectionErrorBoundary>
          <Card>
            <h3 className="text-xs font-bold text-foreground mb-2">{t('intentions.predictions')}</h3>
            <pre className="text-xs text-muted-foreground overflow-x-auto max-h-60">
              {JSON.stringify(predictions, null, 2)}
            </pre>
          </Card>
        </SectionErrorBoundary>
      )}
    </div>
  );
}
