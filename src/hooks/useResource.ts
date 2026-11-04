import { useCallback, useEffect, useState } from 'react';
import { toCommandError } from '../lib/api';
import type { CommandError } from '../lib/types';

export type Resource<T> =
  | { status: 'loading' }
  | { status: 'error'; error: CommandError }
  | { status: 'ready'; data: T };

/**
 * Loads data once per mount (and on `reload()`). `load` must be a stable function,
 * typically a module-level cached loader.
 */
export function useResource<T>(load: () => Promise<T>): { resource: Resource<T>; reload: () => void } {
  const [resource, setResource] = useState<Resource<T>>({ status: 'loading' });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    load().then(
      (data) => {
        if (!cancelled) setResource({ status: 'ready', data });
      },
      (error: unknown) => {
        if (!cancelled) setResource({ status: 'error', error: toCommandError(error) });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [load, attempt]);

  const reload = useCallback(() => {
    setResource({ status: 'loading' });
    setAttempt((value) => value + 1);
  }, []);

  return { resource, reload };
}
