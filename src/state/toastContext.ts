import { createContext, useContext } from 'react';
import type { ToastLevel } from '../lib/notifications';

export interface ToastApi {
  notify: (level: ToastLevel, message: string, title?: string) => void;
  /** Shows the `message` of a CommandError (or any rejection) as an error toast. */
  notifyError: (error: unknown, title?: string) => void;
}

export const ToastContext = createContext<ToastApi | null>(null);

export function useToasts(): ToastApi {
  const context = useContext(ToastContext);
  if (!context) throw new Error('useToasts() must be used inside <ToastProvider>.');
  return context;
}
