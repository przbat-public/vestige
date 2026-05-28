import { Component, type ErrorInfo, type ReactNode } from 'react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import i18n from '@/lib/i18n';

interface Props {
  children: ReactNode;
  fallbackTitle?: string;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class SectionErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    if (import.meta.env.DEV) {
      // biome-ignore lint/suspicious/noConsole: section error diagnostic
      console.error('[vestige] Section error:', error, info.componentStack);
    }
  }

  render() {
    if (this.state.hasError) {
      // Class component can't `useTranslation()`, but the i18next instance
      // is module-level — bind once per render to keep `t()` calls clean.
      // Mirrors the fallback used by the top-level ErrorBoundary.
      const t = i18n.t.bind(i18n);
      return (
        <Alert variant="destructive" className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="font-medium">{this.props.fallbackTitle ?? t('error.sectionFailed')}</div>
            {this.state.error?.message && (
              <div className="text-xs opacity-75 mt-1 truncate">{this.state.error.message}</div>
            )}
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => this.setState({ hasError: false, error: null })}
            className="flex-shrink-0"
          >
            {t('common.retry')}
          </Button>
        </Alert>
      );
    }

    return this.props.children;
  }
}
