import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { api } from '@/stores/api';
import { EVENT, track } from '@/stores/telemetry';
import { toast } from '@/stores/toast';

interface RunDreamCycleButtonProps {
  /**
   * When provided, overrides the default copy on the button — useful
   * for empty-state CTAs ("Run dream cycle to discover hubs" vs the
   * shorter command-palette label).
   */
  labelKey?: string;
  /** Visual variant; `dream` (violet) is the default for marketing emphasis. */
  variant?: 'dream' | 'default' | 'secondary' | 'outline';
  /** sm = empty-state inline; md = standalone button. */
  size?: 'sm' | 'md';
  /** Pre/post hooks for analytics or extra refresh. */
  onSettled?: () => void;
  className?: string;
}

/**
 * Reusable "Run dream cycle" CTA. Wraps `api.dream()` plus the cache
 * invalidation needed to make hubs / insights / decisions / feed
 * refresh once the cycle finishes.
 *
 * The bare `commands.dream` command palette entry already does this,
 * but burying the most impactful maintenance action in ⌘K means new
 * users never discover it. Surfacing the same call in empty-state
 * CTAs is how we close that loop without changing the command palette.
 */
export function RunDreamCycleButton({
  labelKey,
  variant = 'dream',
  size = 'md',
  onSettled,
  className,
}: RunDreamCycleButtonProps) {
  const { t } = useTranslation();
  const qc = useQueryClient();

  const mutation = useMutation({
    mutationFn: () => api.dream(),
    onSuccess: () => {
      toast(t('commands.dreamSuccess'), 'success');
      // Dream cycle materialises hubs/insights/decisions and rewrites
      // dream insights — invalidate every page that surfaces those.
      // We keep this list explicit (rather than nuking the whole
      // cache) so unrelated lists don't refetch.
      qc.invalidateQueries({ queryKey: ['hubs'] });
      qc.invalidateQueries({ queryKey: ['insights'] });
      qc.invalidateQueries({ queryKey: ['decisions'] });
      qc.invalidateQueries({ queryKey: ['briefing'] });
      qc.invalidateQueries({ queryKey: ['stats'] });
    },
    onError: (err: Error) => toast(err.message || t('common.error'), 'error'),
    onSettled,
  });

  return (
    <Button
      type="button"
      variant={variant}
      size={size}
      disabled={mutation.isPending}
      onClick={() => {
        track(EVENT.dream_cycle_run);
        mutation.mutate();
      }}
      className={className}
    >
      {mutation.isPending ? t('common.loading') : t(labelKey ?? 'commands.dream')}
    </Button>
  );
}
