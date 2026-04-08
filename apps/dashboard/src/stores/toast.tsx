import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from 'react';

type ToastType = 'error' | 'success' | 'info';

interface Toast {
  id: number;
  message: string;
  type: ToastType;
}

interface ToastContextValue {
  addToast: (message: string, type?: ToastType) => void;
}

const ToastContext = createContext<ToastContextValue>({
  addToast: () => {
    /* noop default */
  },
});

let globalToastId = 0;

type ToastListener = (message: string, type: ToastType) => void;
let globalListener: ToastListener | null = null;

export function toast(message: string, type: ToastType = 'error') {
  globalListener?.(message, type);
}

export function useToast() {
  return useContext(ToastContext);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const addToast = useCallback((message: string, type: ToastType = 'error') => {
    const id = ++globalToastId;
    setToasts((prev) => [...prev, { id, message, type }]);
    const timer = setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
      timersRef.current.delete(id);
    }, 5000);
    timersRef.current.set(id, timer);
  }, []);

  const removeToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
  }, []);

  useEffect(() => {
    globalListener = addToast;
    return () => {
      globalListener = null;
    };
  }, [addToast]);

  return (
    <ToastContext.Provider value={{ addToast }}>
      {children}
      {toasts.length > 0 && (
        <div className="fixed bottom-4 right-4 z-50 space-y-2 max-w-sm pointer-events-none">
          {toasts.map((t) => (
            <div
              key={t.id}
              className={`pointer-events-auto glass-panel rounded-xl px-4 py-3 text-xs flex items-start gap-2 border-l-2 ${
                t.type === 'error'
                  ? 'border-l-danger text-danger'
                  : t.type === 'success'
                    ? 'border-l-success text-success'
                    : 'border-l-primary text-primary'
              }`}
            >
              <span className="flex-1 break-words">{t.message}</span>
              <button
                type="button"
                onClick={() => removeToast(t.id)}
                className="text-muted-foreground hover:text-foreground flex-shrink-0"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}
    </ToastContext.Provider>
  );
}
