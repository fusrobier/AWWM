// Toast list model (pure). The React provider lives in components/Toasts.tsx.
import type { NoticeLevel } from './types';

export type ToastLevel = NoticeLevel | 'success';

export interface ToastData {
  id: number;
  level: ToastLevel;
  title: string | null;
  message: string;
  /** Incremented when an identical toast is raised again; restarts its timer. */
  revision: number;
}

/** Errors and warnings stay longer so they can be read. */
export const TOAST_DURATION_MS: Record<ToastLevel, number> = {
  info: 5000,
  success: 5000,
  warning: 8000,
  error: 8000,
};

export const MAX_TOASTS = 4;

/**
 * Adds a toast. An identical visible toast is refreshed instead of duplicated, and the
 * oldest non-error toast is dropped when the list is full.
 */
export function addToast(
  list: readonly ToastData[],
  incoming: Omit<ToastData, 'revision'>,
  max = MAX_TOASTS,
): ToastData[] {
  const duplicate = list.find(
    (toast) =>
      toast.level === incoming.level &&
      toast.message === incoming.message &&
      toast.title === incoming.title,
  );
  if (duplicate) {
    return list.map((toast) => (toast === duplicate ? { ...toast, revision: toast.revision + 1 } : toast));
  }
  const next = [...list, { ...incoming, revision: 0 }];
  while (next.length > max) {
    const index = next.findIndex((toast) => toast.level !== 'error');
    next.splice(index === -1 ? 0 : index, 1);
  }
  return next;
}

export function removeToast(list: readonly ToastData[], id: number): ToastData[] {
  return list.filter((toast) => toast.id !== id);
}

/** Errors and warnings are announced assertively, everything else politely. */
export function isUrgent(level: ToastLevel): boolean {
  return level === 'error' || level === 'warning';
}
