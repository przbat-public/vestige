import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { Alert } from './alert';
import { Button } from './button';

interface QueryErrorPanelProps {
  /**
   * The error from a TanStack Query hook (or any thrown value). When omitted,
   * the panel falls back to the generic `common.fetchError` translation.
   */
  error?: unknown;
  /**
   * Callback for the retry button. When omitted, the retry button is hidden.
   * Pass `refetch` from `useQuery` directly.
   */
  onRetry?: () => void;
  /** Optional title override; defaults to `common.fetchError`. */
  title?: string;
  className?: string;
}

function errorMessage(error: unknown): string | null {
  if (!error) return null;
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return null;
}

/**
 * Reusable error panel for TanStack Query failures.
 *
 * Provides a consistent recovery affordance across pages: a localized error
 * message, an optional inline detail (for diagnosability without opening
 * devtools), and a retry button when a refetch handler is supplied.
 *
 * Composition:
 * - Wraps `<Alert variant="destructive">` (already has `role="alert"`)
 * - Renders `<details>` with the underlying error message — keyboard accessible,
 *   screen-reader friendly (announced as a disclosure widget)
 * - Retry button uses `Button` with localized label
 *
 * Usage:
 * ```tsx
 * const { isError, error, refetch } = useQuery(...);
 * if (isError) return <QueryErrorPanel error={error} onRetry={refetch} />;
 * ```
 */
export function QueryErrorPanel({ error, onRetry, title, className }: QueryErrorPanelProps) {
  const { t } = useTranslation();
  const detail = errorMessage(error);
  const heading = title ?? t('common.fetchError');

  return (
    <Alert variant="destructive" className={cn('flex flex-col gap-3', className)}>
      <div className="font-medium">{heading}</div>
      {detail && (
        <details className="text-xs opacity-80">
          <summary className="cursor-pointer select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-red-500/50 rounded">
            {t('common.errorDetails')}
          </summary>
          <pre className="mt-2 whitespace-pre-wrap break-words font-mono">{detail}</pre>
        </details>
      )}
      {onRetry && (
        <div>
          <Button type="button" variant="secondary" size="sm" onClick={onRetry}>
            {t('common.errorRetry')}
          </Button>
        </div>
      )}
    </Alert>
  );
}
