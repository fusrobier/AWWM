import { useCallback, useRef, useState } from 'react';
import { useToasts } from '../state/toastContext';

export type CommandResult<T> = { ok: true; value: T } | { ok: false };

/**
 * Runs backend commands for buttons: tracks which one is busy (by key) and shows the
 * CommandError message in a toast when a command fails.
 */
export function useCommandRunner() {
  const { notifyError } = useToasts();
  const [busyKeys, setBusyKeys] = useState<ReadonlySet<string>>(() => new Set());
  const running = useRef(new Set<string>());

  const run = useCallback(
    async <T>(key: string, command: () => Promise<T>, errorTitle: string): Promise<CommandResult<T>> => {
      if (running.current.has(key)) return { ok: false };
      running.current.add(key);
      setBusyKeys(new Set(running.current));
      try {
        return { ok: true, value: await command() };
      } catch (error) {
        notifyError(error, errorTitle);
        return { ok: false };
      } finally {
        running.current.delete(key);
        setBusyKeys(new Set(running.current));
      }
    },
    [notifyError],
  );

  const isBusy = useCallback((key: string) => busyKeys.has(key), [busyKeys]);

  return { run, isBusy };
}
