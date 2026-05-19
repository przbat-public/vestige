import { QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router';
import App from '@/App';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { queryClient } from '@/stores/query';
import { ToastProvider } from '@/stores/toast';
import { initWebSocket } from '@/stores/websocket';
import '@/lib/i18n';
import './app.css';

// Apply persisted density before React mounts so the first paint already
// reflects the user's preference. The hook below it would set the same
// attribute on mount, but only after the initial render — without this
// pre-mount step the page briefly flashes in default "comfortable" density.
const persistedDensity = localStorage.getItem('vestige-density');
if (persistedDensity === 'compact' || persistedDensity === 'comfortable') {
  document.documentElement.setAttribute('data-density', persistedDensity);
}

const cleanupWs = initWebSocket();
window.addEventListener('unload', cleanupWs);

const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('Missing #root element');
createRoot(rootEl).render(
  <StrictMode>
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter basename="/dashboard">
          <ToastProvider>
            <App />
          </ToastProvider>
        </BrowserRouter>
      </QueryClientProvider>
    </ErrorBoundary>
  </StrictMode>,
);
