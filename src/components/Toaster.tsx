import { useEffect, useRef, useState } from 'react';
import { CircleAlert, CircleCheck, CircleX, Info } from 'lucide-react';
import { useStore } from '../state/store';
import type { Toast } from '../types';
import { IconX } from './Icons';
import { Spinner } from './ui';
import './Toaster.css';

type ToastItem = Toast & { leaving?: boolean };
const EXIT_MS = 400;

/* Toasts are occasional, so they get a full enter and exit: they rise out of
   the bottom edge and leave back through it. The store drops a toast
   immediately, so this keeps a copy mounted until its exit has played. */
export default function Toaster() {
  const { toasts, dismissToast } = useStore();
  const [items, setItems] = useState<ToastItem[]>([]);
  const timers = useRef(new Map<string, number>());

  useEffect(() => {
    setItems((current) => {
      const live = new Map(toasts.map((toast) => [toast.id, toast]));
      const next: ToastItem[] = current.map((item) => live.get(item.id) ?? { ...item, leaving: true });
      for (const toast of toasts) if (!current.some((item) => item.id === toast.id)) next.push(toast);
      return next;
    });
  }, [toasts]);

  useEffect(() => {
    for (const item of items) {
      if (!item.leaving || timers.current.has(item.id)) continue;
      timers.current.set(item.id, window.setTimeout(() => {
        timers.current.delete(item.id);
        setItems((current) => current.filter((entry) => entry.id !== item.id));
      }, EXIT_MS));
    }
  }, [items]);

  useEffect(() => () => timers.current.forEach(window.clearTimeout), []);

  if (items.length === 0) return null;

  return (
    <div className="toaster" role="region" aria-label="Notifications">
      {items.map((toast) => (
        <div key={toast.id} className={`toast toast-${toast.tone}`} data-leaving={toast.leaving || undefined} role="status">
          <span className="toast-icon" aria-hidden="true"><ToastIcon tone={toast.tone} /></span>
          <div className="toast-content">
            <div className="toast-title">{toast.title}</div>
            {toast.detail && <div className="toast-detail">{toast.detail}</div>}
            {toast.progress !== undefined && (
              <div className="toast-progress">
                <div className="toast-progress-fill" style={{ transform: `scaleX(${Math.max(0, Math.min(100, toast.progress)) / 100})` }} />
              </div>
            )}
          </div>
          <button className="toast-close" onClick={() => dismissToast(toast.id)} aria-label="Dismiss notification">
            <IconX size={12} />
          </button>
        </div>
      ))}
    </div>
  );
}

function ToastIcon({ tone }: { tone: Toast['tone'] }) {
  switch (tone) {
    case 'success': return <CircleCheck size={15} />;
    case 'error': return <CircleX size={15} />;
    case 'warning': return <CircleAlert size={15} />;
    case 'progress': return <Spinner size={15} />;
    default: return <Info size={15} />;
  }
}
