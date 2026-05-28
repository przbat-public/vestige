import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

type ToastType = 'error' | 'success' | 'info';

interface ToastAction {
  label: string;
  onAction: () => void;
}

interface ToastOptions {
  /** Auto-dismiss timeout (ms). Defaults to 5000. Use 0 to disable auto-dismiss. */
  duration?: number;
  /** Optional inline action button (e.g. "Undo"). */
  action?: ToastAction;
}

interface Toast {
  id: number;
  message: string;
  type: ToastType;
  action?: ToastAction;
}

interface ToastContextValue {
  addToast: (message: string, type?: ToastType, options?: ToastOptions) => number;
  dismissToast: (id: number) => void;
}

const noopContext: ToastContextValue = {
  addToast: () => 0,
  dismissToast: () => {
    /* noop default */
  },
};

const ToastContext = createContext<ToastContextValue>(noopContext);

let globalToastId = 0;

type ToastListener = (message: string, type: ToastType, options?: ToastOptions) => number;
let globalAddListener: ToastListener | null = null;
let globalDismissListener: ((id: number) => void) | null = null;

/**
 * Fire-and-forget toast. Returns the toast id so callers can cancel/dismiss
 * the toast programmatically (e.g. once a deferred-delete window closes).
 *
 * Returns 0 when no provider is mounted (SSR, tests) — callers should treat
 * 0 as "no live toast" and skip dismissal logic.
 */
export function toast(message: string, type: ToastType = 'error', options?: ToastOptions): number {
  return globalAddListener?.(message, type, options) ?? 0;
}

export function dismissToast(id: number) {
  globalDismissListener?.(id);
}

export function useToast() {
  return useContext(ToastContext);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const removeToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
  }, []);

  const addToast = useCallback((message: string, type: ToastType = 'error', options?: ToastOptions): number => {
    const id = ++globalToastId;
    setToasts((prev) => [...prev, { id, message, type, action: options?.action }]);
    const duration = options?.duration ?? 5000;
    if (duration > 0) {
      const timer = setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
        timersRef.current.delete(id);
      }, duration);
      timersRef.current.set(id, timer);
    }
    return id;
  }, []);

  useEffect(() => {
    globalAddListener = addToast;
    globalDismissListener = removeToast;
    return () => {
      globalAddListener = null;
      globalDismissListener = null;
    };
  }, [addToast, removeToast]);

  return (
    <ToastContext.Provider value={{ addToast, dismissToast: removeToast }}>
      {children}
      {toasts.length > 0 && (
        <section
          aria-live="polite"
          aria-label={t('a11y.notifications')}
          className="fixed bottom-4 right-4 z-50 space-y-2 max-w-sm pointer-events-none"
        >
          {toasts.map((item) => (
            <div
              key={item.id}
              role="status"
              className={`pointer-events-auto glass-panel rounded-xl px-4 py-3 text-xs flex items-start gap-2 border-l-2 ${
                item.type === 'error'
                  ? 'border-l-danger text-danger'
                  : item.type === 'success'
                    ? 'border-l-success text-success'
                    : 'border-l-primary text-primary'
              }`}
            >
              <span className="flex-1 break-words">{item.message}</span>
              {item.action && (
                <button
                  type="button"
                  onClick={() => {
                    item.action?.onAction();
                    removeToast(item.id);
                  }}
                  className="font-medium underline-offset-2 hover:underline flex-shrink-0"
                >
                  {item.action.label}
                </button>
              )}
              <button
                type="button"
                onClick={() => removeToast(item.id)}
                aria-label={t('a11y.dismissNotification')}
                className="text-muted-foreground hover:text-foreground flex-shrink-0"
              >
                ×
              </button>
            </div>
          ))}
        </section>
      )}
    </ToastContext.Provider>
  );
}
