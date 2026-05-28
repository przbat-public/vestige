import { Component, type ErrorInfo, type ReactNode } from 'react';
import i18n from '@/lib/i18n';
import { EVENT, track } from '@/stores/telemetry';

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    if (import.meta.env.DEV) {
      // biome-ignore lint/suspicious/noConsole: error boundary diagnostic
      console.error('[vestige] Uncaught render error:', error, info.componentStack);
    }
    // Fire-and-forget telemetry so post-mortem investigation has a record
    // of which render error tipped the dashboard over. Telemetry sinks
    // to localStorage (see `stores/telemetry.ts`) — never blocks render.
    try {
      track(EVENT.fatal_render_error, {
        message: error.message.slice(0, 240),
        // Component stack can be huge; first 600 chars usually pinpoint
        // the offending tree without bloating the buffer.
        component_stack: (info.componentStack ?? '').slice(0, 600),
        url: window.location.pathname,
      });
    } catch {
      // Telemetry must never throw from a boundary — that would mask the
      // original error and cause an infinite-loop crash.
    }
  }

  render() {
    if (this.state.hasError) {
      const t = i18n.t.bind(i18n);
      return (
        <div className="flex items-center justify-center h-screen bg-background text-foreground">
          <div className="text-center space-y-4 max-w-md p-8">
            <div className="text-4xl">⚠</div>
            <h1 className="text-lg font-semibold text-foreground">{t('error.title')}</h1>
            <p className="text-sm text-muted-foreground">{this.state.error?.message || t('error.fallback')}</p>
            <button
              type="button"
              onClick={() => {
                this.setState({ hasError: false, error: null });
                // Honour the Vite-injected base URL so the dashboard works
                // when mounted under arbitrary paths (e.g. `/foo/dashboard/`
                // behind a reverse proxy). Hardcoding `/dashboard/` would
                // throw a 404 in those deployments.
                window.location.href = import.meta.env.BASE_URL;
              }}
              className="inline-flex items-center justify-center gap-2 rounded-xl text-sm font-medium px-4 py-2 bg-primary/20 text-primary hover:bg-primary/30 transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {t('error.reload')}
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
