import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { errorMessage } from '../lib/backend';
import { cx } from '../lib/format';
import {
  addToast,
  isUrgent,
  removeToast,
  TOAST_DURATION_MS,
  type ToastData,
  type ToastLevel,
} from '../lib/notifications';
import { ToastContext, type ToastApi } from '../state/toastContext';
import { IconButton } from './Button';
import { Icon, type IconName } from './Icon';

const LEVEL_ICONS: Record<ToastLevel, IconName> = {
  info: 'info',
  success: 'success',
  warning: 'warning',
  error: 'error',
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastData[]>([]);
  const nextId = useRef(1);

  const dismiss = useCallback((id: number) => {
    setToasts((list) => removeToast(list, id));
  }, []);

  const api = useMemo<ToastApi>(() => {
    const notify = (level: ToastLevel, message: string, title?: string): void => {
      const id = nextId.current;
      nextId.current += 1;
      setToasts((list) => addToast(list, { id, level, message, title: title ?? null }));
    };
    return {
      notify,
      notifyError: (error, title) => notify('error', errorMessage(error), title),
    };
  }, []);

  const regionRef = useRef<HTMLDivElement>(null);

  // The region is a manual popover so it renders in the top layer. Modal dialogs are in
  // the top layer too; re-showing the popover keeps new toasts above an open dialog.
  useLayoutEffect(() => {
    const region = regionRef.current;
    if (!region || typeof region.showPopover !== 'function') return;
    const dialogOpen = document.querySelector('dialog:modal') !== null;
    if (region.matches(':popover-open')) {
      if (!dialogOpen || toasts.length === 0) return;
      region.hidePopover();
    }
    region.showPopover();
  }, [toasts]);

  const polite = toasts.filter((toast) => !isUrgent(toast.level));
  const urgent = toasts.filter((toast) => isUrgent(toast.level));

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div ref={regionRef} className="toast-region" popover="manual" aria-label="Notifications">
        <ol className="toast-list" aria-live="polite" aria-relevant="additions text">
          {polite.map((toast) => (
            <ToastItem key={toast.id} toast={toast} onDismiss={dismiss} />
          ))}
        </ol>
        <ol className="toast-list" aria-live="assertive" aria-relevant="additions text">
          {urgent.map((toast) => (
            <ToastItem key={toast.id} toast={toast} onDismiss={dismiss} />
          ))}
        </ol>
      </div>
    </ToastContext.Provider>
  );
}

function ToastItem({ toast, onDismiss }: { toast: ToastData; onDismiss: (id: number) => void }) {
  const [paused, setPaused] = useState(false);
  const remaining = useRef(TOAST_DURATION_MS[toast.level]);
  const revision = useRef(toast.revision);

  // Auto-dismiss; the timer pauses while the pointer or keyboard focus is on the toast
  // and restarts when an identical message is raised again.
  useEffect(() => {
    if (revision.current !== toast.revision) {
      revision.current = toast.revision;
      remaining.current = TOAST_DURATION_MS[toast.level];
    }
    if (paused) return undefined;
    const started = Date.now();
    const timer = setTimeout(() => onDismiss(toast.id), remaining.current);
    return () => {
      clearTimeout(timer);
      remaining.current = Math.max(1000, remaining.current - (Date.now() - started));
    };
  }, [paused, toast.id, toast.level, toast.revision, onDismiss]);

  return (
    <li
      className={cx('toast', `toast--${toast.level}`)}
      onPointerEnter={() => setPaused(true)}
      onPointerLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={(event) => {
        const next = event.relatedTarget;
        if (!(next instanceof Node && event.currentTarget.contains(next))) setPaused(false);
      }}
    >
      <Icon name={LEVEL_ICONS[toast.level]} className="toast-icon" />
      <div className="toast-content">
        {toast.title && <p className="toast-title">{toast.title}</p>}
        <p className="toast-message">{toast.message}</p>
      </div>
      <IconButton label="Dismiss notification" icon="close" size="small" onClick={() => onDismiss(toast.id)} />
    </li>
  );
}
